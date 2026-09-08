use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Multipart, Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::domain::calibrator::boxes::{self, BoxField};
use crate::domain::calibrator::pages;
use crate::domain::calibrator::render;
use crate::domain::portal;
use crate::web::portal_auth::require_api;

const RENDER_DPI: f32 = 150.0;

/// Rejects a page filename that isn't a bare `*.pdf` name — this comes
/// straight from the URL path and gets joined onto `pages_dir()`, so it
/// must not be able to smuggle in a path separator or `..`.
fn validate_page_file(file: &str) -> Result<(), Response> {
    let ok = !file.is_empty()
        && !file.contains('/')
        && !file.contains('\\')
        && file != ".."
        && file != "."
        && Path::new(file)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"));
    if ok {
        Ok(())
    } else {
        Err((StatusCode::BAD_REQUEST, "invalid page file name").into_response())
    }
}

fn page_schema_path(pages_dir: &Path, page_file: &str) -> PathBuf {
    pages_dir.join(Path::new(page_file).with_extension("schema.json"))
}

/// Small non-cryptographic suffix so concurrent uploads don't collide
/// on the same staging directory name.
fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn load_page_geometry(page_path: &Path) -> anyhow::Result<forms_core::PageGeometry> {
    let doc = lopdf::Document::load(page_path)?;
    let page_id = *doc
        .get_pages()
        .values()
        .next()
        .ok_or_else(|| anyhow::anyhow!("page file contains no pages"))?;
    forms_core::page_geometry(&doc, page_id)
}

pub async fn list_pages(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_api(portal::ADMIN, &headers, &state) {
        return resp;
    }

    match pages::list_pages() {
        Ok(files) => {
            let names: Vec<String> = files
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .collect();
            Json(names).into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list pages: {err:#}"),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
struct ImportResponse {
    written: Vec<String>,
}

/// Accepts a multipart upload (any single file field) and splits it
/// into per-page PDFs in `pages_dir()`, the same as the old "Open
/// PDF..." button did via a native file picker.
pub async fn import_pdf(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Response {
    if let Err(resp) = require_api(portal::ADMIN, &headers, &state) {
        return resp;
    }

    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        Ok(None) => return (StatusCode::BAD_REQUEST, "no file uploaded").into_response(),
        Err(err) => return (StatusCode::BAD_REQUEST, format!("bad upload: {err}")).into_response(),
    };

    let original_name = field.file_name().unwrap_or("document.pdf").to_string();
    let bytes = match field.bytes().await {
        Ok(bytes) => bytes,
        Err(err) => return (StatusCode::BAD_REQUEST, format!("bad upload: {err}")).into_response(),
    };

    // Staged under a uniquely-named directory (not a uniquely-named
    // file) so the uploaded file keeps its original name — that name's
    // stem is what `import_pdf` uses to name the split-out pages, and
    // it must come from the upload, not from whatever this temp file
    // happens to be called.
    let safe_name = Path::new(&original_name)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "document.pdf".to_string());
    let tmp_dir = std::env::temp_dir().join(format!(
        "admin-calibrator-upload-{}-{}",
        std::process::id(),
        rand_suffix()
    ));
    let tmp_path = tmp_dir.join(&safe_name);

    if let Err(err) = std::fs::create_dir_all(&tmp_dir).and_then(|()| std::fs::write(&tmp_path, &bytes)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to stage upload: {err:#}"),
        )
            .into_response();
    }

    let result = pages::import_pdf(&tmp_path);
    let _ = std::fs::remove_dir_all(&tmp_dir);

    match result {
        Ok(written) => {
            let names = written
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .collect();
            Json(ImportResponse { written: names }).into_response()
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            format!("Failed to import document: {err:#}"),
        )
            .into_response(),
    }
}

pub async fn page_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(file): AxPath<String>,
) -> Response {
    if let Err(resp) = require_api(portal::ADMIN, &headers, &state) {
        return resp;
    }
    if let Err(resp) = validate_page_file(&file) {
        return resp;
    }

    let path = pages::pages_dir().join(&file);
    match render::render_page_png(&path, RENDER_DPI as u32) {
        Ok(bytes) => ([(header::CONTENT_TYPE, "image/png")], bytes).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to render page: {err:#}"),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
struct FieldsResponse {
    fields: Vec<BoxField>,
}

/// Loads whatever fields are already saved for this page (from its own
/// `<page>.schema.json`, if present) so reopening a page you've
/// already calibrated shows your existing boxes instead of a blank
/// slate.
pub async fn get_fields(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(file): AxPath<String>,
) -> Response {
    if let Err(resp) = require_api(portal::ADMIN, &headers, &state) {
        return resp;
    }
    if let Err(resp) = validate_page_file(&file) {
        return resp;
    }

    let schema_path = page_schema_path(&pages::pages_dir(), &file);
    let fields = match forms_core::Schema::load(&schema_path) {
        Ok(schema) => schema
            .pages
            .iter()
            .find(|p| p.file == file)
            .map(|page_info| {
                schema
                    .fields
                    .iter()
                    .filter(|f| f.page_file == file)
                    .map(|f| boxes::from_field(f, RENDER_DPI, page_info.height_pt))
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    Json(FieldsResponse { fields }).into_response()
}

#[derive(Deserialize)]
pub struct SaveFieldsRequest {
    fields: Vec<BoxField>,
}

#[derive(Serialize)]
struct SaveFieldsResponse {
    saved_fields: usize,
}

/// Saves this page's boxes into its own `<page>.schema.json`, next to
/// the page's PDF — one schema file per page, matching the file this
/// page's fields have always lived in.
pub async fn save_fields(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(file): AxPath<String>,
    Json(body): Json<SaveFieldsRequest>,
) -> Response {
    if let Err(resp) = require_api(portal::ADMIN, &headers, &state) {
        return resp;
    }
    if let Err(resp) = validate_page_file(&file) {
        return resp;
    }

    let pages_dir = pages::pages_dir();
    let page_path = pages_dir.join(&file);
    let geometry = match load_page_geometry(&page_path) {
        Ok(g) => g,
        Err(err) => {
            return (StatusCode::BAD_REQUEST, format!("Failed to read {file}: {err:#}")).into_response();
        }
    };

    let fields: Vec<forms_core::Field> = body
        .fields
        .iter()
        .filter(|b| !b.field_key.trim().is_empty())
        .map(|b| boxes::to_field(b, RENDER_DPI, geometry.display_height, &file))
        .collect();

    let schema = forms_core::Schema {
        source: file.clone(),
        pages: vec![forms_core::PageInfo {
            file: file.clone(),
            width_pt: geometry.display_width,
            height_pt: geometry.display_height,
        }],
        fields,
    };

    let out_path = page_schema_path(&pages_dir, &file);
    match schema.save(&out_path) {
        Ok(()) => Json(SaveFieldsResponse {
            saved_fields: schema.fields.len(),
        })
        .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save {}: {err:#}", out_path.display()),
        )
            .into_response(),
    }
}
