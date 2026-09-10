use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};

use crate::AppState;
use crate::domain::{network, qr, sign_in};

pub async fn lobby_page() -> Html<&'static str> {
    Html(include_str!("../../templates/lobby.html"))
}

/// Built from the request's own `Host` header, so the QR code normally
/// points at whatever address the lobby screen's own browser used to
/// reach this server — which is necessarily an address a customer's
/// phone on the same network can reach too.
///
/// The exception is when the lobby display was opened via a
/// self-only host — `localhost`/`127.0.0.1`, or `0.0.0.0` (easy to
/// end up with since that's what the server's own bind address looks
/// like) — none of which mean anything to a phone. In that case we
/// substitute this machine's real LAN IP instead.
fn base_url(headers: &HeaderMap) -> String {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:3000");

    if network::is_self_only_host(host) {
        let port = host.rsplit_once(':').map(|(_, p)| p).unwrap_or("3000");
        if let Some(ip) = network::local_lan_ip() {
            return format!("http://{ip}:{port}");
        }
    }

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

/// The staff portal-picker's URL, for the "CST: go to ..." line on
/// the lobby display. Computed the same protected way as the QR
/// code's own URL — not from the browser's `window.location.origin`
/// client-side — so it can't end up showing `0.0.0.0` or `localhost`
/// either if the lobby display was opened via one of those.
pub async fn staff_url(headers: HeaderMap) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "url": format!("{}/staff", base_url(&headers)) }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_keeps_a_lan_host_as_is() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "192.168.1.42:3000".parse().unwrap());
        assert_eq!(base_url(&headers), "http://192.168.1.42:3000");
    }

    #[test]
    fn base_url_swaps_localhost_for_the_real_lan_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "localhost:3000".parse().unwrap());
        let url = base_url(&headers);
        // Can't assert the exact IP (depends on the test machine's
        // network), but it must no longer say localhost/127.0.0.1 —
        // that's the whole bug this guards against.
        assert!(!url.contains("localhost"));
        assert!(!url.contains("127.0.0.1"));
        assert!(url.ends_with(":3000"));
    }

    /// Regression guard for the actual bug reported: the server's own
    /// startup log says "listening on http://0.0.0.0:3000", and
    /// opening *that* address bakes a useless 0.0.0.0 into the QR
    /// code and the URL shown under it — a phone can never connect to
    /// 0.0.0.0, that address only means "every interface" to a
    /// listening socket, not "this machine" to a caller.
    #[test]
    fn base_url_swaps_0_0_0_0_for_the_real_lan_ip() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "0.0.0.0:3000".parse().unwrap());
        let url = base_url(&headers);
        assert!(!url.contains("0.0.0.0"));
        assert!(url.ends_with(":3000"));
    }
}
