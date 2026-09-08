use axum::extract::{Path as AxPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::domain::portal::Portal;
use crate::domain::queue::{self, QueueEntry, QueueId};
use crate::web::portal_auth::{require_api, require_page};

/// CST, Neets, and LRA all show the exact same "customers waiting"
/// table — this module is the one implementation, mounted three times
/// (see `router`) with a different [`Portal`]/[`QueueId`] pair each
/// time, instead of copy-pasting the page/list/remove handlers per
/// portal.
async fn queue_page(portal: Portal, State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_page(portal, &headers, &state) {
        return resp;
    }
    Html(include_str!("../../templates/queue.html")).into_response()
}

async fn list_api(
    portal: Portal,
    queue_id: QueueId,
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(resp) = require_api(portal, &headers, &state) {
        return resp;
    }
    match queue::list(queue_id) {
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
    queue_id: QueueId,
    State(state): State<AppState>,
    headers: HeaderMap,
    AxPath(entry_id): AxPath<i64>,
) -> Response {
    if let Err(resp) = require_api(portal, &headers, &state) {
        return resp;
    }

    let _guard = state.queue_lock.lock().unwrap();
    match queue::remove(queue_id, entry_id) {
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
/// password. Mount under `/{portal.id}` alongside
/// `portal_auth::router(portal)`.
pub fn router(portal: Portal, queue_id: QueueId) -> Router<AppState> {
    Router::new()
        .route(
            "/queue",
            get(move |state: State<AppState>, headers: HeaderMap| queue_page(portal, state, headers)),
        )
        .route(
            "/api/queue",
            get(move |state: State<AppState>, headers: HeaderMap| list_api(portal, queue_id, state, headers)),
        )
        .route(
            "/api/queue/{id}/remove",
            post(
                move |state: State<AppState>, headers: HeaderMap, path: AxPath<i64>| {
                    remove_api(portal, queue_id, state, headers, path)
                },
            ),
        )
}

#[derive(Deserialize)]
struct CheckinRequest {
    name: String,
    description: String,
}

#[derive(Serialize)]
struct CheckinResponse {
    entry: QueueEntry,
}

/// A customer checking in isn't logging into the staff portal, so
/// unlike everything else in this module this is deliberately *not*
/// gated by `require_api` — only viewing and managing a queue requires
/// that portal's password. Nothing calls this yet (the customer-facing
/// page that would is still to come) but it's independently usable and
/// tested now.
async fn checkin_api(
    State(state): State<AppState>,
    AxPath(portal_id): AxPath<String>,
    Json(body): Json<CheckinRequest>,
) -> Response {
    let Some(queue_id) = QueueId::from_portal_id(&portal_id) else {
        return (StatusCode::NOT_FOUND, "unknown queue").into_response();
    };
    if body.name.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "name is required").into_response();
    }

    let _guard = state.queue_lock.lock().unwrap();
    match queue::checkin(queue_id, body.name.clone(), body.description.clone()) {
        Ok(entry) => Json(CheckinResponse { entry }).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to check in: {err:#}"),
        )
            .into_response(),
    }
}

/// The public (unauthenticated) `/api/queue/{portal_id}/checkin`
/// route, shared by all three queues — mount once at the top level,
/// not nested under any single portal's router.
pub fn public_checkin_router() -> Router<AppState> {
    Router::new().route("/api/queue/{portal_id}/checkin", post(checkin_api))
}
