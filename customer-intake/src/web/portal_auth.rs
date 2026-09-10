use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;

use crate::AppState;
use crate::domain::portal::Portal;
use crate::web::cookies;

fn cookie_name(portal: Portal) -> String {
    format!("{}_session", portal.id)
}

pub fn is_authenticated(portal: Portal, headers: &HeaderMap, state: &AppState) -> bool {
    let Some(token) = cookies::read(headers, &cookie_name(portal)) else {
        return false;
    };
    state.sessions.is_valid(portal.id, &token)
}

/// Gate for a portal *page* route: redirects to that portal's login
/// page instead of the requested content when there's no valid
/// session.
pub fn require_page(portal: Portal, headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    if is_authenticated(portal, headers, state) {
        Ok(())
    } else {
        Err(Redirect::to(&format!("/{}/login", portal.id)).into_response())
    }
}

/// Gate for a portal *API* route: a plain 401 instead of a redirect,
/// since these are called from `fetch`, not navigated to.
pub fn require_api(portal: Portal, headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    if is_authenticated(portal, headers, state) {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "portal session required").into_response())
    }
}

async fn root(portal: Portal, State(state): State<AppState>, headers: HeaderMap) -> Response {
    if is_authenticated(portal, &headers, &state) {
        Redirect::to(portal.home_path).into_response()
    } else {
        Redirect::to(&format!("/{}/login", portal.id)).into_response()
    }
}

/// Same static shell for every portal — it reads its own portal id
/// back out of `window.location.pathname` client-side to know which
/// display name to show and which path to `POST` to.
async fn login_page(portal: Portal, State(state): State<AppState>, headers: HeaderMap) -> Response {
    if is_authenticated(portal, &headers, &state) {
        return Redirect::to(portal.home_path).into_response();
    }
    Html(include_str!("../../templates/portal-login.html")).into_response()
}

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

async fn login_submit(portal: Portal, State(state): State<AppState>, Form(form): Form<LoginForm>) -> Response {
    match state.sessions.login(&portal, &form.password) {
        Some(token) => {
            let mut response = Redirect::to(portal.home_path).into_response();
            let cookie = cookies::set(&cookie_name(portal), &token);
            response.headers_mut().insert(header::SET_COOKIE, cookie.parse().unwrap());
            response
        }
        None => Redirect::to(&format!("/{}/login?error=1", portal.id)).into_response(),
    }
}

async fn logout(portal: Portal, State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookies::read(&headers, &cookie_name(portal)) {
        state.sessions.logout(portal.id, &token);
    }

    let mut response = Redirect::to(&format!("/{}/login", portal.id)).into_response();
    let cleared = cookies::clear(&cookie_name(portal));
    response.headers_mut().insert(header::SET_COOKIE, cleared.parse().unwrap());
    response
}

/// The login/logout/root-redirect routes shared by every portal.
/// Mount under `/{portal.id}` alongside whatever page/API routes that
/// portal actually offers.
pub fn router(portal: Portal) -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(move |state: State<AppState>, headers: HeaderMap| root(portal, state, headers)),
        )
        .route(
            "/login",
            get(move |state: State<AppState>, headers: HeaderMap| login_page(portal, state, headers)).post(
                move |state: State<AppState>, form: Form<LoginForm>| login_submit(portal, state, form),
            ),
        )
        .route(
            "/logout",
            post(move |state: State<AppState>, headers: HeaderMap| logout(portal, state, headers)),
        )
}
