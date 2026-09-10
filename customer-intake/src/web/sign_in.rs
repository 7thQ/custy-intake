use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;

use crate::AppState;
use crate::domain::sign_in::{self, NewSignIn};
use crate::web::cookies;

const SESSION_COOKIE: &str = "signin_session";

/// The token identifying an in-progress sign-in, if the request came
/// with one. Doesn't check it against the database — callers that
/// need to know it's still a *live* pending sign-in should also call
/// `sign_in::pending_exists`.
pub fn session_token(headers: &HeaderMap) -> Option<String> {
    cookies::read(headers, SESSION_COOKIE)
}

pub async fn sign_in_page() -> Html<&'static str> {
    Html(include_str!("../../templates/sign-in.html"))
}

#[derive(Deserialize)]
pub struct SignInRequest {
    name: String,
    rank: Option<String>,
    squadron: String,
    reason_for_visit: String,
    ticket_number: Option<String>,
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Starts a sign-in session: validates the required fields server-side
/// (the client also does this live, but a request could always skip
/// the UI), writes the pending sign-in, and hands back a session
/// cookie every later step of the flow rides on.
pub async fn submit(State(state): State<AppState>, Json(body): Json<SignInRequest>) -> Response {
    let Some(name) = non_empty(&body.name) else {
        return (StatusCode::BAD_REQUEST, "name is required").into_response();
    };
    let Some(squadron) = non_empty(&body.squadron) else {
        return (StatusCode::BAD_REQUEST, "squadron is required").into_response();
    };
    let Some(reason_for_visit) = non_empty(&body.reason_for_visit) else {
        return (StatusCode::BAD_REQUEST, "reason for visit is required").into_response();
    };

    let input = NewSignIn {
        name,
        rank: body.rank.as_deref().and_then(non_empty),
        squadron,
        reason_for_visit,
        ticket_number: body.ticket_number.as_deref().and_then(non_empty),
    };

    match sign_in::create(&state.db, input).await {
        Ok(token) => {
            let mut response = StatusCode::OK.into_response();
            let cookie = cookies::set(SESSION_COOKIE, &token);
            response.headers_mut().insert(header::SET_COOKIE, cookie.parse().unwrap());
            response
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to sign in: {err:#}")).into_response(),
    }
}
