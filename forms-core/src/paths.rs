use std::path::PathBuf;

/// Where every imported document page, and each page's own
/// `<page>.schema.json`, lives: a workspace-level `pages/` folder, so
/// both the customer-facing intake flow and the admin calibrator always
/// agree on the same physical folder without either needing to be told
/// where the other put its files.
///
/// Anchored at this crate's own manifest dir rather than at whichever
/// binary happens to call it, since `env!("CARGO_MANIFEST_DIR")`
/// resolves per-crate at compile time — resolving it here means every
/// binary in the workspace lands on the same path.
pub fn pages_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../pages"))
}
