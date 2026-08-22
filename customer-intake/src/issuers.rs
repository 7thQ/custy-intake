use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

/// One person who can issue equipment. `name` stands in for a real
/// signature (we can't render an actual signature image), while
/// `name_grade_org` is the fuller "Name, Grade, Org" line the form
/// prints separately.
#[derive(Debug, Clone, Deserialize)]
pub struct Issuer {
    pub name: String,
    pub name_grade_org: String,
}

/// Hardcoded, plain, editable file — not compiled in — so the real
/// roster can be edited (or replaced) without touching Rust code.
pub fn issuers_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/issuers.json"))
}

pub fn load_issuers() -> Result<Vec<Issuer>> {
    let path = issuers_path();
    let bytes =
        std::fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}
