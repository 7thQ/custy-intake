use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};

use crate::AppState;
use crate::domain::{qr, sign_in};

pub async fn lobby_page() -> Html<&'static str> {
    Html(include_str!("../../templates/lobby.html"))
}

/// Built from the request's own `Host` header rather than a hardcoded
/// config value, so the QR code always points at whatever address the
/// lobby screen's own browser used to reach this server — which is
/// necessarily an address a customer's phone on the same network can
/// reach too.
fn base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:3000");
    format!("http://{host}")
}

pub async fn sign_in_qr(headers: HeaderMap) -> Response {
    let url = format!("{}/sign-in", base_url(&headers));
    match qr::svg_for(&url) {
        Ok(svg) => ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to render QR code: {err:#}"),
        )
            .into_response(),
    }
}

/// The unauthenticated general queue — everyone currently waiting,
/// across every department, for the lobby display to show. Read-only:
/// only the password-gated portals can remove someone.
pub async fn general_queue(State(state): State<AppState>) -> Response {
    match sign_in::list_queue(&state.db, None).await {
        Ok(entries) => Json(entries).into_response(),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load queue: {err:#}")).into_response(),
    }
}
