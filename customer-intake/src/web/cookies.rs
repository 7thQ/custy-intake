use axum::http::HeaderMap;
use axum::http::header;

/// Reads a single cookie's value out of the request's `Cookie` header.
/// Shared by portal-session cookies and the sign-in session cookie —
/// same parsing either way.
pub fn read(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|pair| {
        let pair = pair.trim();
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

/// A `Set-Cookie` header value for `name=value`, valid site-wide.
pub fn set(name: &str, value: &str) -> String {
    format!("{name}={value}; Path=/; HttpOnly; SameSite=Lax")
}

/// A `Set-Cookie` header value that immediately expires `name`.
pub fn clear(name: &str) -> String {
    format!("{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}
