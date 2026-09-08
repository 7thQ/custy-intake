pub mod domain;
pub mod web;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::routing::get;
use tower_http::services::ServeDir;

use domain::portal;
use domain::queue::QueueId;

/// Shared server state. `sessions` covers every portal (admin, CST,
/// Neets, LRA) — see [`domain::portal::Sessions`] — and `queue_lock`
/// serializes the read-modify-write file updates behind check-in/
/// remove, since (unlike `domain::storage`'s one-file-per-submission
/// writes) a queue's entries all live in one shared JSON file.
#[derive(Clone)]
pub struct AppState {
    pub sessions: portal::Sessions,
    pub queue_lock: Arc<Mutex<()>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: portal::Sessions::new(),
            queue_lock: Arc::new(Mutex::new(())),
        }
    }
}

/// Assembles the full app router. Split out from [`run`] so tests can
/// build and drive it directly (via `tower::ServiceExt::oneshot`)
/// without binding a real TCP listener.
pub fn build_router(state: AppState) -> Router {
    let static_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");

    Router::new()
        .route("/", get(web::home::home_page))
        .route("/issue-receipt", get(web::issue_receipt::issue_receipt_page))
        .route("/api/issue-receipt/init", get(web::issue_receipt::issue_receipt_init))
        .route(
            "/api/issue-receipt/submit",
            axum::routing::post(web::issue_receipt::issue_receipt_submit),
        )
        .merge(web::queue::public_checkin_router())
        .nest("/admin", admin_router())
        .nest("/cst", portal_router(portal::CST, QueueId::Cst))
        .nest("/neets", portal_router(portal::NEETS, QueueId::Neets))
        .nest("/lra", portal_router(portal::LRA, QueueId::Lra))
        .nest_service("/static", ServeDir::new(static_dir))
        .with_state(state)
}

fn admin_router() -> Router<AppState> {
    web::portal_auth::router(portal::ADMIN)
        .route("/dashboard", get(web::admin::dashboard_page))
        .route("/calibrator", get(web::admin::calibrator_page))
        .route("/calibrator/page/{file}", get(web::admin::calibrator_editor_page))
        .route("/api/pages", get(web::admin::calibrate::list_pages))
        .route("/api/pages/import", axum::routing::post(web::admin::calibrate::import_pdf))
        .route("/api/pages/{file}/image", get(web::admin::calibrate::page_image))
        .route(
            "/api/pages/{file}/fields",
            get(web::admin::calibrate::get_fields).post(web::admin::calibrate::save_fields),
        )
}

/// The CST/Neets/LRA portals are identical: generic login/logout plus
/// the generic queue page and API, just pointed at a different
/// [`portal::Portal`]/[`QueueId`] pair.
fn portal_router(portal: portal::Portal, queue_id: QueueId) -> Router<AppState> {
    web::portal_auth::router(portal).merge(web::queue::router(portal, queue_id))
}

pub async fn run() -> anyhow::Result<()> {
    let app = build_router(AppState::default());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("CST Customer Intake listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
