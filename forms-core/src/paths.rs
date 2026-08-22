use std::path::PathBuf;

/// Where every imported document page, and each page's own
/// `<page>.schema.json`, lives. Hardcoded so both `doc-calibrator` and
/// `customer-intake` always agree on the same physical folder, without
/// either one having to be told where the other put its files.
///
/// Anchored at this crate's own manifest dir rather than at whichever
/// binary happens to call it, since `env!("CARGO_MANIFEST_DIR")`
/// resolves per-crate at compile time — resolving it here means both
/// binaries land on the same path regardless of which one is built.
pub fn pages_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../doc-calibrator/pages"))
}
