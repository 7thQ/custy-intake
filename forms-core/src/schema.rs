use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FieldKind {
    Text,
    Checkbox,
}

/// A box on a page, in PDF point space (origin at the page's bottom-left,
/// y-up), described by its center, size, and rotation. Rotation exists
/// because scanned pages are rarely perfectly straight.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RotatedRect {
    pub cx: f32,
    pub cy: f32,
    pub width: f32,
    pub height: f32,
    /// Degrees, counter-clockwise, 0 = axis-aligned.
    pub rotation_deg: f32,
}

/// One text box on one page. `field_key` is the join key: the
/// customer-intake app writes `fields[field_key] = value` in a
/// submission, and every `Field` sharing that `field_key` (possibly on
/// different pages, or even different documents) gets filled with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub field_key: String,
    pub label: String,
    pub page_file: String,
    pub kind: FieldKind,
    pub rect: RotatedRect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    pub file: String,
    pub width_pt: f32,
    pub height_pt: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schema {
    pub source: String,
    pub pages: Vec<PageInfo>,
    pub fields: Vec<Field>,
}

impl Schema {
    pub fn load(path: &Path) -> Result<Schema> {
        let bytes =
            fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        fs::write(path, bytes).with_context(|| format!("failed to write {}", path.display()))
    }
}
