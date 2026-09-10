use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::AppState;
use crate::domain::portal::Portal;
use crate::domain::sign_in;
use crate::web::portal_auth::{require_api, require_page};

/// CST, Neets, and LRA all show the exact same queue table, just
/// filtered to their own department; Admin's Queue Control is the same
/// thing again with no filter at all. One implementation, mounted four
/// times (see [`router`]) instead of copy-pasted.
async fn queue_page(portal: Portal, State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_page(portal, &headers, &state) {
        return resp;
    }
    Html(include_str!("../../templates/queue.html")).into_response()
}

async fn list_api(
    portal: Portal,
    department: Option<&'static str>,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_api(portal, &headers, &state) {
        return resp;
    }
    match sign_in::list_queue(&state.db, department).await {
        Ok(entries) => Json(entries).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list queue: {err:#}"),
        )
            .into_response(),
    }
}

async fn remove_api(
    portal: Portal,
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(session_token): AxPath<String>,
) -> Response {
    if let Err(resp) = require_api(portal, &headers, &state) {
        return resp;
    }

    match sign_in::remove_from_queue(&state.db, &session_token, portal.id).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "no such queue entry").into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to remove entry: {err:#}"),
        )
            .into_response(),
    }
}

/// The queue page plus its list/remove API, gated by `portal`'s
/// password. `department` filters the list — `Some("cst")` etc. for a
/// department portal, `None` for Admin's all-departments view. Mount
/// under `/{portal.id}` alongside `portal_auth::router(portal)`.
pub fn router(portal: Portal, department: Option<&'static str>) -> Router<AppState> {
    Router::new()
        .route(
            "/queue",
            get(move |state: State<AppState>, headers: HeaderMap| queue_page(portal, state, headers)),
        )
        .route(
            "/api/queue",
            get(move |state: State<AppState>, headers: HeaderMap| {
                list_api(portal, department, state, headers)
            }),
        )
        .route(
            "/api/queue/{token}/remove",
            post(
                move |state: State<AppState>, headers: HeaderMap, path: AxPath<String>| {
                    remove_api(portal, state, headers, path)
                },
            ),
        )
}
