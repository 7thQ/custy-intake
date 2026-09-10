use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Serialize;
use std::collections::BTreeMap;

use crate::AppState;
use crate::domain::sign_in;
use crate::domain::{issue_receipt, issuers};
use crate::web::sign_in as web_sign_in;

/// Filling out a service form only makes sense mid sign-in-flow, so
/// this (and the submit handler below) both require a still-pending
/// sign-in session — the same one `/sign-in` created.
async fn require_pending_session(headers: &HeaderMap, state: &AppState) -> Result<String, Response> {
    let Some(token) = web_sign_in::session_token(headers) else {
        return Err(Redirect::to("/sign-in").into_response());
    };
    match sign_in::pending_exists(&state.db, &token).await {
        Ok(true) => Ok(token),
        Ok(false) => Err(Redirect::to("/sign-in").into_response()),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to check sign-in session: {err:#}"),
        )
            .into_response()),
    }
}

pub async fn issue_receipt_page(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(resp) = require_pending_session(&headers, &state).await {
        return resp;
    }
    Html(include_str!("../../templates/issue-receipt.html")).into_response()
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

/// Attaches this service's answers to the pending sign-in — the
/// moment this succeeds, the customer appears on the queue — then
/// fills the PDF best-effort on top of that. The ticket is captured
/// either way, so `ok` stays true even if the PDF fill fails; that
/// failure just surfaces in `message`.
pub async fn issue_receipt_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SubmitRequest>,
) -> Response {
    let token = match require_pending_session(&headers, &state).await {
        Ok(token) => token,
        Err(resp) => return resp,
    };

    let fields_json = match serde_json::to_string(&payload.fields) {
        Ok(json) => json,
        Err(err) => {
            return Json(SubmitResponse {
                ok: false,
                message: format!("Failed to encode submitted fields: {err:#}"),
            })
            .into_response();
        }
    };

    match sign_in::submit_issue_receipt(&state.db, &token, &fields_json).await {
        Ok(()) => {
            let message = match issue_receipt::fill_completed_pdf(&token, &payload.fields) {
                Ok(pdf_dir) => format!("You're on the queue. Filled form saved in {}", pdf_dir.display()),
                Err(err) => format!("You're on the queue, but failed to fill the PDF: {err:#}"),
            };
            Json(SubmitResponse { ok: true, message }).into_response()
        }
        Err(err) => Json(SubmitResponse {
            ok: false,
            message: format!("Failed to submit: {err:#}"),
        })
        .into_response(),
    }
}
