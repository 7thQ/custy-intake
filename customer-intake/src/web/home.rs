use axum::response::Html;

pub async fn home_page() -> Html<&'static str> {
    Html(include_str!("../../templates/index.html"))
}
