pub mod domain;
pub mod firewall;
pub mod web;

use anyhow::Context;
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
        .route("/api/staff-url", get(web::lobby::staff_url))
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
    // Blocking (waits on terminal input for the sudo password), so it
    // runs off the async runtime rather than stalling a tokio worker.
    let guard = tokio::task::spawn_blocking(firewall::FirewallGuard::open)
        .await
        .context("firewall setup task panicked")??;

    let db = domain::db::connect().await?;
    let state = AppState {
        sessions: portal::Sessions::new(),
        db,
    };
    let app = build_router(state);

    // 0.0.0.0, not 127.0.0.1: customer phones on the building wifi need
    // to reach this server too, not just this machine talking to itself.
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", firewall::PORT)).await?;

    // Deliberately NOT `listener.local_addr()` here — for a 0.0.0.0
    // bind that just reports back "0.0.0.0:PORT", which looks like an
    // address but isn't one a phone (or anything else) can actually
    // connect to. Printing that invites exactly the bug this caused:
    // opening the lobby display at http://0.0.0.0:3000 and having
    // that useless address end up baked into the QR code.
    match domain::network::local_lan_ip() {
        Some(ip) => println!("CST Customer Intake listening — open http://{ip}:{} on this network.", firewall::PORT),
        None => println!(
            "CST Customer Intake listening on port {} (couldn't detect this machine's LAN IP — open it via this machine's actual network address, not 0.0.0.0).",
            firewall::PORT
        ),
    }
    println!("Press Ctrl+C to stop — this closes the firewall port back up first.");

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(wait_for_ctrl_c())
        .await;

    // Always roll the firewall rule back, however the server stopped
    // (clean Ctrl+C or an error), not just on the happy path.
    tokio::task::spawn_blocking(move || guard.close()).await.ok();

    result.map_err(Into::into)
}

async fn wait_for_ctrl_c() {
    let _ = tokio::signal::ctrl_c().await;
    println!("\nShutting down...");
}
