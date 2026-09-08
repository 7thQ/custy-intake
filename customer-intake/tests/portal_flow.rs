//! Router-level tests for the generic portal auth + queue system,
//! driven directly through `tower::ServiceExt::oneshot` (no real TCP
//! listener needed — this is exactly what moving the app into a
//! library crate buys us).
//!
//! Each test that mutates a queue file sticks to one portal (and
//! cleans up after itself) so tests running concurrently don't race
//! on the same `queues/<portal>.json`.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use customer_intake::{AppState, build_router};
use serde_json::Value;
use tower::ServiceExt;

fn app() -> axum::Router {
    build_router(AppState::default())
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn session_cookie(response: &axum::response::Response) -> String {
    let raw = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("login response should set a session cookie")
        .to_str()
        .unwrap();
    raw.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn unauthenticated_pages_redirect_to_their_own_login() {
    for path in ["/admin/dashboard", "/cst/queue", "/neets/queue", "/lra/queue"] {
        let response = app()
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
async fn wrong_password_redirects_with_error_and_sets_no_cookie() {
    let response = app()
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
async fn cst_login_checkin_list_remove_round_trip() {
    let app = app();

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/cst/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("password=cstP0tal"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(login_response.status().is_redirection());
    assert_eq!(login_response.headers().get(header::LOCATION).unwrap(), "/cst/queue");
    let cookie = session_cookie(&login_response);

    // A customer checking in doesn't need the CST password — this is
    // the public endpoint.
    let checkin_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/queue/cst/checkin")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"name":"SSgt Test Customer","description":"integration test checkin"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(checkin_response.status(), StatusCode::OK);
    let checkin_body = body_json(checkin_response).await;
    let entry_id = checkin_body["entry"]["id"].as_str().unwrap().to_string();

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/cst/api/queue")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let entries = body_json(list_response).await;
    assert!(
        entries.as_array().unwrap().iter().any(|e| e["id"] == entry_id.as_str()),
        "checked-in entry should appear in the queue list"
    );

    let remove_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/cst/api/queue/{entry_id}/remove"))
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(remove_response.status(), StatusCode::OK);

    let list_after_remove = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/cst/api/queue")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let entries_after = body_json(list_after_remove).await;
    assert!(!entries_after.as_array().unwrap().iter().any(|e| e["id"] == entry_id.as_str()));
}

#[tokio::test]
async fn lra_checkin_and_list_round_trip() {
    let app = app();

    let checkin_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/queue/lra/checkin")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"LRA Test Customer","description":"lra flow test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(checkin_response.status(), StatusCode::OK);
    let entry_id = body_json(checkin_response).await["entry"]["id"].as_str().unwrap().to_string();

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/lra/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("password=LRAP0rtal"))
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = session_cookie(&login_response);

    let list_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/lra/api/queue")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let entries = body_json(list_response).await;
    assert!(entries.as_array().unwrap().iter().any(|e| e["id"] == entry_id.as_str()));

    // Clean up so re-running the suite doesn't accumulate entries.
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/lra/api/queue/{entry_id}/remove"))
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn sessions_do_not_cross_portals() {
    let app = app();

    let login_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/neets/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("password=Neets"))
                .unwrap(),
        )
        .await
        .unwrap();
    let neets_cookie = session_cookie(&login_response);

    // A valid Neets session must not authenticate the LRA API.
    let response = app
        .oneshot(
            Request::builder()
                .uri("/lra/api/queue")
                .header(header::COOKIE, &neets_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn checkin_rejects_unknown_portal() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/queue/bogus/checkin")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"x","description":"y"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
