pub mod calibrate;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};

use crate::AppState;
use crate::domain::portal;
use crate::web::portal_auth::require_page;

pub async fn dashboard_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_page(portal::ADMIN, &headers, &state) {
        return resp;
    }
    Html(include_str!("../../../templates/admin/dashboard.html")).into_response()
}

pub async fn calibrator_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_page(portal::ADMIN, &headers, &state) {
        return resp;
    }
    Html(include_str!("../../../templates/admin/calibrator.html")).into_response()
}

/// The page filename lives in the URL path only so the client-side
/// editor can read it back out (via `window.location`) to drive its
/// own API calls — this handler just gates access and serves the same
/// static editor shell for every page.
pub async fn calibrator_editor_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_page(portal::ADMIN, &headers, &state) {
        return resp;
    }
    Html(include_str!("../../../templates/admin/calibrator-page.html")).into_response()
}
