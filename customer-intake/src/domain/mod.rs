//! Pure application logic: file storage, PDF schema/fill wiring, and
//! session/queue bookkeeping. Nothing in this module tree knows about
//! HTTP, axum, or cookies — that translation happens in `crate::web`.
//! Keeping the split means every rule here is unit-testable on its
//! own, and the same logic could be driven by a different frontend
//! (a CLI, a different web framework) without rewriting it.

pub mod calibrator;
pub mod issue_receipt;
pub mod issuers;
pub mod portal;
pub mod queue;
pub mod storage;
