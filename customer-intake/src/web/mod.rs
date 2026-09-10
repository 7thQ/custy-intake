//! HTTP layer: axum handlers, request/response DTOs, and template/
//! static-asset wiring. Translates HTTP concerns (cookies, JSON
//! bodies, multipart uploads) into calls onto `crate::domain`, which
//! is where the actual rules live.

pub mod admin;
pub mod cookies;
pub mod issue_receipt;
pub mod lobby;
pub mod portal_auth;
pub mod queue;
pub mod services;
pub mod sign_in;
pub mod staff;
