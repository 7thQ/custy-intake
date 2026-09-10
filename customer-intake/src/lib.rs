pub mod domain;
pub mod web;

use axum::Router;
use axum::routing::{get, post};
use sqlx::SqlitePool;
use tower_http::services::ServeDir;

use domain::portal;

/// Shared server state. `sessions` covers every password-gated portal
/// (admin, CST, Neets, LRA) — see [`domain::portal::Sessions`]. `db`
/// is the one SQLite pool backing sign-ins, the queue, and every
/// service's submitted data.
#[derive(Clone)]
pub struct AppState {
    pub sessions: portal::Sessions,
    pub db: SqlitePool,
}

/// Assembles the full app router. Split out from [`run`] so tests can
/// build and drive it directly (via `tower::ServiceExt::oneshot`)
/// without binding a real TCP listener.
pub fn build_router(state: AppState) -> Router {
    let static_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/static");

    Router::new()
        .route("/", get(web::lobby::lobby_page))
        .route("/api/qr/sign-in", get(web::lobby::sign_in_qr))
        .route("/api/queue", get(web::lobby::general_queue))
        .route(
            "/sign-in",
            get(web::sign_in::sign_in_page),
        )
        .route("/api/sign-in", post(web::sign_in::submit))
        .route("/services", get(web::services::services_page))
        .route("/staff", get(web::staff::staff_page))
        .route("/issue-receipt", get(web::issue_receipt::issue_receipt_page))
        .route("/api/issue-receipt/init", get(web::issue_receipt::issue_receipt_init))
        .route("/api/issue-receipt/submit", post(web::issue_receipt::issue_receipt_submit))
        .nest("/admin", admin_router())
        .nest("/cst", portal_router(portal::CST, Some("cst")))
        .nest("/neets", portal_router(portal::NEETS, Some("neets")))
        .nest("/lra", portal_router(portal::LRA, Some("lra")))
        .nest_service("/static", ServeDir::new(static_dir))
        .with_state(state)
}

fn admin_router() -> Router<AppState> {
    web::portal_auth::router(portal::ADMIN)
        .merge(web::queue::router(portal::ADMIN, None))
        .route("/dashboard", get(web::admin::dashboard_page))
        .route("/calibrator", get(web::admin::calibrator_page))
        .route("/calibrator/page/{file}", get(web::admin::calibrator_editor_page))
        .route("/api/pages", get(web::admin::calibrate::list_pages))
        .route("/api/pages/import", post(web::admin::calibrate::import_pdf))
        .route("/api/pages/{file}/image", get(web::admin::calibrate::page_image))
        .route(
            "/api/pages/{file}/fields",
            get(web::admin::calibrate::get_fields).post(web::admin::calibrate::save_fields),
        )
}

/// The CST/Neets/LRA portals are identical: generic login/logout plus
/// the generic queue page and API, just pointed at a different
/// [`portal::Portal`]/department pair.
fn portal_router(portal: portal::Portal, department: Option<&'static str>) -> Router<AppState> {
    web::portal_auth::router(portal).merge(web::queue::router(portal, department))
}

pub async fn run() -> anyhow::Result<()> {
    let db = domain::db::connect().await?;
    let state = AppState {
        sessions: portal::Sessions::new(),
        db,
    };
    let app = build_router(state);

    // 0.0.0.0, not 127.0.0.1: customer phones on the building wifi need
    // to reach this server too, not just this machine talking to itself.
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    println!("CST Customer Intake listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    Ok(())
}
