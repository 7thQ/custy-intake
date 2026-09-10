//! Router-level tests for the sign-in -> service -> queue flow and the
//! generic portal auth, driven directly through
//! `tower::ServiceExt::oneshot` against an in-memory database — no
//! real TCP listener or database file needed. Each test gets its own
//! fresh in-memory database, so nothing needs cleaning up between
//! runs.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use customer_intake::domain::db::connect_in_memory;
use customer_intake::domain::portal::Sessions;
use customer_intake::{AppState, build_router};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn app() -> Router {
    let state = AppState {
        sessions: Sessions::new(),
        db: connect_in_memory().await,
    };
    build_router(state)
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn session_cookie(response: &axum::response::Response) -> String {
    let raw = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("response should set a session cookie")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().to_string()
}

async fn login(app: &Router, portal_id: &str, password: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/{portal_id}/login"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "password={}",
                    urlencoding_lite(password)
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    session_cookie(&response)
}

/// Just enough percent-encoding for the one special character
/// (`#`) that shows up in a hardcoded portal password in these tests.
fn urlencoding_lite(value: &str) -> String {
    value.replace('#', "%23")
}

async fn sign_in(app: &Router, name: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sign-in")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "name": name,
                        "rank": "SSgt",
                        "squadron": "375th CS",
                        "reason_for_visit": "Laptop won't boot",
                        "ticket_number": null,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    session_cookie(&response)
}

async fn submit_issue_receipt(app: &Router, sign_in_cookie: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/issue-receipt/submit")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, sign_in_cookie)
                .body(Body::from(json!({"fields": {"ticket_number": "TCK-1"}}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn unauthenticated_portal_pages_redirect_to_their_own_login() {
    let app = app().await;
    for path in ["/admin/dashboard", "/cst/queue", "/neets/queue", "/lra/queue"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(response.status().is_redirection(), "{path} should redirect when unauthenticated");

        let portal_id = path.split('/').nth(1).unwrap();
        let location = response.headers().get(header::LOCATION).unwrap().to_str().unwrap();
        assert_eq!(location, format!("/{portal_id}/login"));
    }
}

#[tokio::test]
async fn wrong_portal_password_redirects_with_error_and_sets_no_cookie() {
    let app = app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("password=not-the-password"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(response.status().is_redirection());
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/admin/login?error=1"
    );
    assert!(response.headers().get(header::SET_COOKIE).is_none());
}

#[tokio::test]
async fn sessions_do_not_cross_portals() {
    let app = app().await;
    let cst_cookie = login(&app, "cst", "cstP0tal").await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/neets/api/queue")
                .header(header::COOKIE, &cst_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn services_and_issue_receipt_require_a_pending_sign_in() {
    let app = app().await;
    for path in ["/services", "/issue-receipt"] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            response.status().is_redirection(),
            "{path} should redirect without a sign-in session"
        );
        assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/sign-in");
    }
}

#[tokio::test]
async fn sign_in_rejects_missing_required_fields() {
    let app = app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sign-in")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"name": "", "squadron": "375th CS", "reason_for_visit": "x"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn not_on_any_queue_until_service_submitted() {
    let app = app().await;
    let cookie = sign_in(&app, "Jane Doe").await;

    // Signed in, so /services is reachable now...
    let services_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/services")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(services_response.status(), StatusCode::OK);

    // ...but nobody's on the general queue until a service is submitted.
    let queue = body_json(
        app.oneshot(Request::builder().uri("/api/queue").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    assert!(queue.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn full_flow_sign_in_submit_appear_on_queue_then_remove() {
    let app = app().await;
    let sign_in_cookie = sign_in(&app, "Jane Doe").await;

    let submit_response = submit_issue_receipt(&app, &sign_in_cookie).await;
    assert_eq!(submit_response.status(), StatusCode::OK);
    assert_eq!(body_json(submit_response).await["ok"], true);

    // Now on the general queue...
    let general = body_json(
        app.clone()
            .oneshot(Request::builder().uri("/api/queue").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    let general_entries = general.as_array().unwrap();
    assert_eq!(general_entries.len(), 1);
    assert_eq!(general_entries[0]["display_name"], "SSgt Jane Doe");
    assert_eq!(general_entries[0]["department"], "cst");

    // ...and on CST's own queue, but not Neets'.
    let cst_cookie = login(&app, "cst", "cstP0tal").await;
    let cst_queue = body_json(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/cst/api/queue")
                    .header(header::COOKIE, &cst_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    let cst_entries = cst_queue.as_array().unwrap();
    assert_eq!(cst_entries.len(), 1);
    let token = cst_entries[0]["session_token"].as_str().unwrap().to_string();

    let neets_cookie = login(&app, "neets", "Neets").await;
    let neets_queue = body_json(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/neets/api/queue")
                    .header(header::COOKIE, &neets_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await;
    assert!(neets_queue.as_array().unwrap().is_empty());

    // CST removes them (appointment done).
    let remove_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/cst/api/queue/{token}/remove"))
                .header(header::COOKIE, &cst_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remove_response.status(), StatusCode::OK);

    // Gone from the queue now...
    let general_after = body_json(
        app.clone()
            .oneshot(Request::builder().uri("/api/queue").body(Body::empty()).unwrap())
            .await
            .unwrap(),
    )
    .await;
    assert!(general_after.as_array().unwrap().is_empty());

    // ...and a second removal is a no-op 404, not an error.
    let second_remove = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/cst/api/queue/{token}/remove"))
                .header(header::COOKIE, &cst_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second_remove.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_queue_control_sees_every_department() {
    let app = app().await;
    let sign_in_cookie = sign_in(&app, "Contractor").await;
    submit_issue_receipt(&app, &sign_in_cookie).await;

    let admin_cookie = login(&app, "admin", "Cst#1Shop").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/api/queue")
                .header(header::COOKIE, &admin_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let entries = body_json(response).await;
    assert_eq!(entries.as_array().unwrap().len(), 1);
}
