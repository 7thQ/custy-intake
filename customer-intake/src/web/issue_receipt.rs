use axum::Json;
use axum::response::Html;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::domain::{issue_receipt, issuers, storage};

pub async fn issue_receipt_page() -> Html<&'static str> {
    Html(include_str!("../../templates/issue-receipt.html"))
}

#[derive(Serialize)]
pub struct ApplyOption {
    field_key: String,
    label: String,
}

#[derive(Serialize)]
pub struct InitResponse {
    issuers: Vec<issuers::Issuer>,
    issuers_error: Option<String>,
    extra_fields: Vec<issue_receipt::ExtraField>,
    schema_error: Option<String>,
    date_of_issue: String,
    apply_options: Vec<ApplyOption>,
    crossed_out_marker: String,
}

/// Everything the Issue Receipt page needs to render itself: the
/// issuer roster, today's date, the fixed apply-options list, the
/// strikethrough marker value, and any schema fields nobody's written
/// a dedicated row for yet.
pub async fn issue_receipt_init() -> Json<InitResponse> {
    let (extra_fields, schema_error) = match issue_receipt::load_fields() {
        Ok(fields) => (issue_receipt::extra_fields(&fields), None),
        Err(err) => (
            Vec::new(),
            Some(format!(
                "Failed to load form fields from {}: {err:#}",
                issue_receipt::schema_path().display()
            )),
        ),
    };

    let (issuers, issuers_error) = match issuers::load_issuers() {
        Ok(list) => (list, None),
        Err(err) => (Vec::new(), Some(format!("Failed to load issuer list: {err:#}"))),
    };

    Json(InitResponse {
        issuers,
        issuers_error,
        extra_fields,
        schema_error,
        date_of_issue: chrono::Local::now().format("%Y-%m-%d").to_string(),
        apply_options: issue_receipt::APPLY_OPTIONS
            .iter()
            .map(|(field_key, label)| ApplyOption {
                field_key: field_key.to_string(),
                label: label.to_string(),
            })
            .collect(),
        crossed_out_marker: issue_receipt::CROSSED_OUT.to_string(),
    })
}

#[derive(serde::Deserialize)]
pub struct SubmitRequest {
    fields: BTreeMap<String, String>,
}

#[derive(Serialize)]
pub struct SubmitResponse {
    ok: bool,
    message: String,
}

/// Mirrors the old Submit button: the submission record is what
/// matters most and is saved first; filling the PDF is best-effort on
/// top of that. Either way the ticket is captured, so `ok` stays true
/// even if the PDF fill fails — that failure just surfaces in
/// `message` — and the client returns to Home the same way the app
/// used to.
pub async fn issue_receipt_submit(Json(payload): Json<SubmitRequest>) -> Json<SubmitResponse> {
    let id = storage::new_submission_id();
    match storage::save_submission(id, "temporary_issue_receipt", payload.fields.clone()) {
        Ok(json_path) => {
            let message = match issue_receipt::fill_completed_pdf(id, &payload.fields) {
                Ok(pdf_dir) => format!(
                    "Saved ticket ({}) and filled form in {}",
                    json_path.display(),
                    pdf_dir.display()
                ),
                Err(err) => format!(
                    "Saved ticket ({}) but failed to fill the PDF: {err:#}",
                    json_path.display()
                ),
            };
            Json(SubmitResponse { ok: true, message })
        }
        Err(err) => Json(SubmitResponse {
            ok: false,
            message: format!("Failed to save ticket: {err:#}"),
        }),
    }
}
