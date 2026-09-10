use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::AppState;
use crate::domain::sign_in;
use crate::web::sign_in as web_sign_in;

/// The "pick a service" screen (chat assistant + service list) —
/// reachable only mid sign-in-flow, after `/sign-in` has been
/// completed. Landing here without a pending session sends the
/// customer back to start one.
pub async fn services_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(token) = web_sign_in::session_token(&headers) else {
        return Redirect::to("/sign-in").into_response();
    };

    match sign_in::pending_exists(&state.db, &token).await {
        Ok(true) => Html(include_str!("../../templates/services.html")).into_response(),
        Ok(false) => Redirect::to("/sign-in").into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to check sign-in session: {err:#}"),
        )
            .into_response(),
    }
}
