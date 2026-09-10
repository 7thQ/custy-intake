use axum::response::Html;

/// The Admin/CST/Neets/LRA link list — reached via the small "For CSTs
/// only" QR code on the lobby display instead of a visible menu.
/// Public: every link it offers is itself password-gated, so nothing
/// here needs protecting.
pub async fn staff_page() -> Html<&'static str> {
    Html(include_str!("../../templates/staff.html"))
}
