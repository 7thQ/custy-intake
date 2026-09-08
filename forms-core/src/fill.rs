use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

use crate::geometry::page_geometry;
use crate::schema::{Field, FieldKind, Schema};

/// Fills every page referenced by `schema` with values from `values`
/// (keyed by `field_key`), reading source pages from `pages_dir` and
/// writing filled copies into `out_dir`. Source page files are never
/// modified.
pub fn fill_pages(
    pages_dir: &Path,
    schema: &Schema,
    values: &BTreeMap<String, String>,
    out_dir: &Path,
) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let mut written = Vec::new();

    for page in &schema.pages {
        let src_path = pages_dir.join(&page.file);
        let mut doc = Document::load(&src_path)
            .with_context(|| format!("failed to load {}", src_path.display()))?;

        let page_id = *doc
            .get_pages()
            .values()
            .next()
            .context("page file contains no pages")?;

        let mut field_ops = Vec::new();
        for field in schema.fields.iter().filter(|f| f.page_file == page.file) {
            let Some(value) = values.get(&field.field_key) else {
                continue;
            };
            if value.trim().is_empty() {
                continue;
            }
            field_ops.extend(field_operations(field, value));
        }

        if !field_ops.is_empty() {
            // Field coordinates are authored in "display space" (matching
            // the rasterized preview the calibrator shows), so every
            // page's `/Rotate` quirk is corrected for once here via a
            // single `cm`, rather than every caller having to reason
            // about raw MediaBox rotation.
            let geometry = page_geometry(&doc, page_id)?;
            ensure_font_resource(&mut doc, page_id)?;

            let mut ops = vec![
                Operation::new("q", vec![]),
                Operation::new(
                    "cm",
                    geometry.correction.iter().copied().map(Object::from).collect(),
                ),
            ];
            ops.extend(field_ops);
            ops.push(Operation::new("Q", vec![]));

            append_content(&mut doc, page_id, ops)?;
        }

        let out_path = out_dir.join(&page.file);
        doc.save(&out_path)
            .with_context(|| format!("failed to save {}", out_path.display()))?;
        written.push(out_path);
    }

    Ok(written)
}

fn field_operations(field: &Field, value: &str) -> Vec<Operation> {
    if value == STRIKETHROUGH {
        return strike_through_operations(field);
    }

    let rect = field.rect;
    let theta = rect.rotation_deg.to_radians();
    let (sin, cos) = theta.sin_cos();

    let (text, font_size) = match field.kind {
        FieldKind::Text => (value.to_string(), estimate_font_size(value, rect.width, rect.height)),
        FieldKind::Checkbox => {
            if !is_checked(value) {
                return vec![];
            }
            ("X".to_string(), (rect.height * 0.8).clamp(6.0, 24.0))
        }
    };

    // Anchor point in the box's own (unrotated) local space, then rotate
    // and translate it into page space so the text matrix and the box
    // agree on the same rotation.
    let local_x = -rect.width / 2.0 + 2.0;
    let local_y = -(font_size * 0.35);
    let tx = rect.cx + local_x * cos - local_y * sin;
    let ty = rect.cy + local_x * sin + local_y * cos;

    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), font_size.into()]),
        Operation::new(
            "Tm",
            vec![
                cos.into(),
                sin.into(),
                (-sin).into(),
                cos.into(),
                tx.into(),
                ty.into(),
            ],
        ),
        Operation::new("Tj", vec![Object::string_literal(text)]),
        Operation::new("ET", vec![]),
    ]
}

/// Value convention: a Text or Checkbox field whose value is exactly
/// this marker is rendered as a thick struck-through line instead of
/// literal text — for printed yes/no or circle-one style choices on a
/// form (e.g. "Yes / No", "Reimage / Add to Domain / ...") where the
/// selected option should look crossed out. A run of plain dash glyphs
/// at normal font weight reads as nearly invisible at typical box
/// sizes, so this draws an actual vector line instead of relying on
/// font rendering.
pub const STRIKETHROUGH: &str = "------";

/// Draws a thick horizontal line through the middle of the field's box,
/// rotated the same way text in that box would be.
fn strike_through_operations(field: &Field) -> Vec<Operation> {
    let rect = field.rect;
    let theta = rect.rotation_deg.to_radians();
    let (sin, cos) = theta.sin_cos();

    let half_w = (rect.width / 2.0 - 2.0).max(1.0);
    let rotate = |lx: f32, ly: f32| -> (f32, f32) {
        (rect.cx + lx * cos - ly * sin, rect.cy + lx * sin + ly * cos)
    };
    let (x1, y1) = rotate(-half_w, 0.0);
    let (x2, y2) = rotate(half_w, 0.0);

    let stroke_width = (rect.height * 0.2).clamp(2.0, 5.0);

    vec![
        Operation::new("q", vec![]),
        Operation::new("RG", vec![0.0.into(), 0.0.into(), 0.0.into()]),
        Operation::new("w", vec![stroke_width.into()]),
        Operation::new("m", vec![x1.into(), y1.into()]),
        Operation::new("l", vec![x2.into(), y2.into()]),
        Operation::new("S", vec![]),
        Operation::new("Q", vec![]),
    ]
}

fn is_checked(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "true" | "yes" | "y" | "1" | "on" | "checked" | "x"
    )
}

/// Rough Helvetica width heuristic (no embedded font metrics available):
/// shrink the font until the estimated string width fits the box, within
/// a sane range.
fn estimate_font_size(value: &str, width_pt: f32, height_pt: f32) -> f32 {
    let mut font_size = (height_pt * 0.7).clamp(6.0, 18.0);
    let avg_char_width_factor = 0.52;
    let usable_width = (width_pt - 4.0).max(1.0);

    while font_size > 5.0 {
        let estimated_width = value.chars().count() as f32 * font_size * avg_char_width_factor;
        if estimated_width <= usable_width {
            break;
        }
        font_size -= 0.5;
    }

    font_size
}

fn ensure_font_resource(doc: &mut Document, page_id: ObjectId) -> Result<()> {
    let resources_val = doc
        .get_object(page_id)?
        .as_dict()?
        .get(b"Resources")
        .ok()
        .cloned();

    let resources_id: ObjectId = match resources_val {
        Some(Object::Reference(id)) => id,
        Some(Object::Dictionary(d)) => {
            let id = doc.add_object(Object::Dictionary(d));
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Resources", Object::Reference(id));
            id
        }
        _ => {
            let id = doc.add_object(Object::Dictionary(Dictionary::new()));
            doc.get_object_mut(page_id)?
                .as_dict_mut()?
                .set("Resources", Object::Reference(id));
            id
        }
    };

    let has_f1 = doc
        .get_object(resources_id)
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"Font").ok())
        .and_then(|f| f.as_dict().ok())
        .map(|fonts| fonts.has(b"F1"))
        .unwrap_or(false);

    if !has_f1 {
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });

        let res_dict = doc.get_object_mut(resources_id)?.as_dict_mut()?;
        let mut fonts = match res_dict.get(b"Font") {
            Ok(Object::Dictionary(f)) => f.clone(),
            _ => Dictionary::new(),
        };
        fonts.set("F1", Object::Reference(font_id));
        res_dict.set("Font", Object::Dictionary(fonts));
    }

    Ok(())
}

fn append_content(doc: &mut Document, page_id: ObjectId, ops: Vec<Operation>) -> Result<()> {
    let encoded = Content { operations: ops }.encode()?;
    let content_id = doc.add_object(Stream::new(dictionary! {}, encoded));

    let page_dict = doc.get_object_mut(page_id)?.as_dict_mut()?;
    let new_contents = match page_dict.get(b"Contents") {
        Ok(Object::Array(arr)) => {
            let mut arr = arr.clone();
            arr.push(Object::Reference(content_id));
            Object::Array(arr)
        }
        Ok(Object::Reference(id)) => {
            Object::Array(vec![Object::Reference(*id), Object::Reference(content_id)])
        }
        _ => Object::Reference(content_id),
    };
    page_dict.set("Contents", new_contents);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{PageInfo, RotatedRect};
    use std::collections::BTreeMap;

    #[test]
    fn fills_real_page_with_rotation_correction() {
        let pages_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../pages");
        let schema = Schema {
            source: "customer_forms.pdf".into(),
            pages: vec![PageInfo {
                file: "customer_forms-page-1.pdf".into(),
                width_pt: 612.0,
                height_pt: 792.0,
            }],
            fields: vec![
                Field {
                    id: "poc_name_box".into(),
                    field_key: "poc_name".into(),
                    label: "POC Name".into(),
                    page_file: "customer_forms-page-1.pdf".into(),
                    kind: FieldKind::Text,
                    rect: RotatedRect {
                        cx: 300.0,
                        cy: 522.0,
                        width: 200.0,
                        height: 16.0,
                        rotation_deg: 0.0,
                    },
                },
                Field {
                    id: "ticket_number_box".into(),
                    field_key: "ticket_number".into(),
                    label: "Ticket Number".into(),
                    page_file: "customer_forms-page-1.pdf".into(),
                    kind: FieldKind::Text,
                    rect: RotatedRect {
                        cx: 400.0,
                        cy: 463.0,
                        width: 150.0,
                        height: 16.0,
                        rotation_deg: 6.0,
                    },
                },
            ],
        };

        let mut values = BTreeMap::new();
        values.insert("poc_name".to_string(), "Jane Doe".to_string());
        values.insert("ticket_number".to_string(), "TCK-9001".to_string());

        let out_dir = std::env::temp_dir().join("forms-core-fill-test");
        let written = fill_pages(&pages_dir, &schema, &values, &out_dir).unwrap();
        assert_eq!(written.len(), 1);
        assert!(written[0].exists());
    }
}

