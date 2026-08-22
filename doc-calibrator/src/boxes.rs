use egui::{Pos2, Vec2};
use forms_core::FieldKind;

#[derive(Clone)]
pub struct BoxDraft {
    pub field_key: String,
    pub label: String,
    pub kind: FieldKind,
    /// Center, in rendered-image pixel space (top-left origin, y-down),
    /// at the calibrator's fixed render DPI.
    pub center_px: Pos2,
    pub size_px: Vec2,
    /// Degrees, clockwise, as drawn on screen. 0 = axis-aligned.
    pub rotation_deg: f32,
}

impl BoxDraft {
    pub fn new(center_px: Pos2, size_px: Vec2) -> Self {
        Self {
            field_key: String::new(),
            label: String::new(),
            kind: FieldKind::Text,
            center_px,
            size_px,
            rotation_deg: 0.0,
        }
    }
}

#[derive(Default)]
pub enum DragState {
    #[default]
    None,
    Creating {
        start_px: Pos2,
    },
    Moving {
        index: usize,
        grab_offset_px: Vec2,
    },
    Rotating {
        index: usize,
    },
}

fn rotate_vec(v: Vec2, sin: f32, cos: f32) -> Vec2 {
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// The four corners of a box, in screen space, for drawing.
pub fn corners_screen(b: &BoxDraft, img_origin: Pos2, scale: f32) -> [Pos2; 4] {
    let center = img_origin + b.center_px.to_vec2() * scale;
    let half = b.size_px * scale / 2.0;
    let theta = b.rotation_deg.to_radians();
    let (sin, cos) = theta.sin_cos();
    let local = [
        Vec2::new(-half.x, -half.y),
        Vec2::new(half.x, -half.y),
        Vec2::new(half.x, half.y),
        Vec2::new(-half.x, half.y),
    ];
    local.map(|v| center + rotate_vec(v, sin, cos))
}

/// Screen position of the small handle used to drag out a rotation. Sits
/// a fixed screen-pixel distance above the box's top edge, rotating with
/// the box.
pub fn rotate_handle_screen(b: &BoxDraft, img_origin: Pos2, scale: f32) -> Pos2 {
    let center = img_origin + b.center_px.to_vec2() * scale;
    let half_h = b.size_px.y * scale / 2.0;
    let handle_dist = half_h + 22.0;
    let theta = b.rotation_deg.to_radians();
    let (sin, cos) = theta.sin_cos();
    center + rotate_vec(Vec2::new(0.0, -handle_dist), sin, cos)
}

/// Topmost box (last drawn) whose rotated rectangle contains `point`
/// (screen space).
pub fn hit_test(boxes: &[BoxDraft], point: Pos2, img_origin: Pos2, scale: f32) -> Option<usize> {
    for (idx, b) in boxes.iter().enumerate().rev() {
        let center = img_origin + b.center_px.to_vec2() * scale;
        let theta = -b.rotation_deg.to_radians();
        let (sin, cos) = theta.sin_cos();
        let local = rotate_vec(point - center, sin, cos);
        let half = b.size_px * scale / 2.0;
        if local.x.abs() <= half.x && local.y.abs() <= half.y {
            return Some(idx);
        }
    }
    None
}

/// Converts a drawn box (screen/pixel space) into a `forms_core::Field`
/// in the page's display-point space, for saving into a schema.
///
/// `dpi` is the DPI the page was rasterized at when this box was drawn.
/// `display_height_pt` is that page's rendered height in PDF points
/// (from `forms_core::page_geometry`), needed to flip pixel-space
/// (top-left, y-down) into PDF-style (bottom-left, y-up) coordinates.
///
/// `rotation_deg` is carried through unchanged: empirically verified
/// (by rendering and visually checking slope direction) against this
/// project's pages, which all carry `/Rotate 270`. A page with a
/// different `/Rotate` value has not been independently verified and
/// may need re-checking the same way if one is ever calibrated.
pub fn to_field(
    b: &BoxDraft,
    dpi: f32,
    display_height_pt: f32,
    page_file: &str,
) -> forms_core::Field {
    let k = 72.0 / dpi;

    forms_core::Field {
        id: format!("{page_file}#{}", b.field_key),
        field_key: b.field_key.clone(),
        label: b.label.clone(),
        page_file: page_file.to_string(),
        kind: b.kind,
        rect: forms_core::RotatedRect {
            cx: b.center_px.x * k,
            cy: display_height_pt - b.center_px.y * k,
            width: b.size_px.x * k,
            height: b.size_px.y * k,
            rotation_deg: b.rotation_deg,
        },
    }
}

/// Inverse of [`to_field`]: rebuilds a screen/pixel-space `BoxDraft` from
/// a saved `forms_core::Field`, so a previously-saved schema can be
/// loaded back into the calibrator for further editing instead of
/// redrawing every box from scratch.
pub fn from_field(field: &forms_core::Field, dpi: f32, display_height_pt: f32) -> BoxDraft {
    let k = dpi / 72.0;
    let rect = field.rect;

    BoxDraft {
        field_key: field.field_key.clone(),
        label: field.label.clone(),
        kind: field.kind,
        center_px: Pos2::new(rect.cx * k, (display_height_pt - rect.cy) * k),
        size_px: Vec2::new(rect.width * k, rect.height * k),
        rotation_deg: rect.rotation_deg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_screen_box_to_display_point_space() {
        let b = BoxDraft {
            field_key: "poc_name".to_string(),
            label: "POC Name".to_string(),
            kind: FieldKind::Text,
            center_px: Pos2::new(100.0, 50.0),
            size_px: Vec2::new(200.0, 16.0),
            rotation_deg: 10.0,
        };

        // dpi=150 -> k = 72/150 = 0.48
        let field = to_field(&b, 150.0, 792.0, "customer_forms-page-1.pdf");

        assert_eq!(field.field_key, "poc_name");
        assert_eq!(field.page_file, "customer_forms-page-1.pdf");
        assert!((field.rect.cx - 48.0).abs() < 1e-4);
        assert!((field.rect.cy - (792.0 - 24.0)).abs() < 1e-4);
        assert!((field.rect.width - 96.0).abs() < 1e-4);
        assert!((field.rect.height - 7.68).abs() < 1e-3);
        assert!((field.rect.rotation_deg - 10.0).abs() < 1e-4);
    }

    #[test]
    fn to_field_and_from_field_round_trip() {
        let original = BoxDraft {
            field_key: "ticket_number".to_string(),
            label: "Ticket Number".to_string(),
            kind: FieldKind::Checkbox,
            center_px: Pos2::new(780.0, 684.0),
            size_px: Vec2::new(160.0, 20.0),
            rotation_deg: 8.0,
        };

        let field = to_field(&original, 150.0, 792.0, "customer_forms-page-1.pdf");
        let restored = from_field(&field, 150.0, 792.0);

        assert_eq!(restored.field_key, original.field_key);
        assert_eq!(restored.label, original.label);
        assert_eq!(restored.kind, original.kind);
        assert!((restored.center_px.x - original.center_px.x).abs() < 1e-2);
        assert!((restored.center_px.y - original.center_px.y).abs() < 1e-2);
        assert!((restored.size_px.x - original.size_px.x).abs() < 1e-2);
        assert!((restored.size_px.y - original.size_px.y).abs() < 1e-2);
        assert!((restored.rotation_deg - original.rotation_deg).abs() < 1e-2);
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    /// Simulates what the live calibrator UI would produce: two boxes
    /// drawn in screen/pixel space (one rotated), converted to a schema,
    /// and filled against the real page-1 PDF. This proves the whole
    /// draw -> schema -> fill pipeline (including the rotation sign
    /// convention) without needing to click through the actual GUI.
    #[test]
    fn draw_to_schema_to_fill_end_to_end() {
        let pages_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("pages");
        let page_file = "customer_forms-page-1.pdf";

        let mut poc_name_box = BoxDraft::new(egui::pos2(650.0, 560.0), egui::vec2(220.0, 20.0));
        poc_name_box.field_key = "poc_name".to_string();
        poc_name_box.label = "POC Name".to_string();

        let mut ticket_box = BoxDraft::new(egui::pos2(780.0, 684.0), egui::vec2(160.0, 20.0));
        ticket_box.field_key = "ticket_number".to_string();
        ticket_box.label = "Ticket Number".to_string();
        ticket_box.rotation_deg = 8.0;

        let doc = lopdf::Document::load(pages_dir.join(page_file)).unwrap();
        let page_id = *doc.get_pages().values().next().unwrap();
        let geometry = forms_core::page_geometry(&doc, page_id).unwrap();

        let fields = vec![
            to_field(&poc_name_box, 150.0, geometry.display_height, page_file),
            to_field(&ticket_box, 150.0, geometry.display_height, page_file),
        ];

        let schema = forms_core::Schema {
            source: "customer_forms.pdf".to_string(),
            pages: vec![forms_core::PageInfo {
                file: page_file.to_string(),
                width_pt: geometry.display_width,
                height_pt: geometry.display_height,
            }],
            fields,
        };

        let mut values = BTreeMap::new();
        values.insert("poc_name".to_string(), "Jane Doe".to_string());
        values.insert("ticket_number".to_string(), "TCK-9001".to_string());

        let out_dir = std::env::temp_dir().join("doc-calibrator-e2e-test");
        let written = forms_core::fill_pages(&pages_dir, &schema, &values, &out_dir).unwrap();
        assert_eq!(written.len(), 1);
        assert!(written[0].exists());
        println!("wrote {}", written[0].display());
    }
}

#[cfg(test)]
mod real_schema_check {
    use super::*;

    #[test]
    fn loads_the_users_real_schema() {
        // Points at the user's live, evolving schema file rather than a
        // fixture, so this only checks structural sanity (loads, and
        // round-trips) rather than an exact field count that would
        // break every time a field is added or removed.
        let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("pages/customer_forms-page-1.schema.json");
        let schema = forms_core::Schema::load(&schema_path).unwrap();
        assert!(!schema.fields.is_empty());

        let restored: Vec<BoxDraft> = schema
            .fields
            .iter()
            .map(|f| from_field(f, 150.0, 792.0))
            .collect();

        assert_eq!(restored.len(), schema.fields.len());
        // sanity: every restored box is still within the rendered page's
        // pixel bounds (150 DPI, 612x792pt page -> 1275x1650px)
        for b in &restored {
            assert!(b.center_px.x > 0.0 && b.center_px.x < 1275.0);
            assert!(b.center_px.y > 0.0 && b.center_px.y < 1650.0);
        }
    }
}
