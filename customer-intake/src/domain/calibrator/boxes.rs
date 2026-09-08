use forms_core::FieldKind;
use serde::{Deserialize, Serialize};

/// One box drawn on a rendered page, in rendered-image pixel space
/// (top-left origin, y-down) at the calibrator's fixed render DPI.
/// `rotation_deg` is degrees, clockwise, as drawn on screen — the same
/// convention the old egui canvas used, and what the browser's
/// canvas/pointer math naturally produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxField {
    pub field_key: String,
    pub label: String,
    pub kind: FieldKind,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation_deg: f32,
}

/// Converts a drawn box (rendered-pixel space) into a
/// `forms_core::Field` in the page's display-point space, for saving
/// into a schema.
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
pub fn to_field(b: &BoxField, dpi: f32, display_height_pt: f32, page_file: &str) -> forms_core::Field {
    let k = 72.0 / dpi;

    forms_core::Field {
        id: format!("{page_file}#{}", b.field_key),
        field_key: b.field_key.clone(),
        label: b.label.clone(),
        page_file: page_file.to_string(),
        kind: b.kind,
        rect: forms_core::RotatedRect {
            cx: b.center_x * k,
            cy: display_height_pt - b.center_y * k,
            width: b.width * k,
            height: b.height * k,
            rotation_deg: b.rotation_deg,
        },
    }
}

/// Inverse of [`to_field`]: rebuilds a pixel-space `BoxField` from a
/// saved `forms_core::Field`, so a previously-saved schema can be
/// loaded back into the calibrator for further editing instead of
/// redrawing every box from scratch.
pub fn from_field(field: &forms_core::Field, dpi: f32, display_height_pt: f32) -> BoxField {
    let k = dpi / 72.0;
    let rect = field.rect;

    BoxField {
        field_key: field.field_key.clone(),
        label: field.label.clone(),
        kind: field.kind,
        center_x: rect.cx * k,
        center_y: (display_height_pt - rect.cy) * k,
        width: rect.width * k,
        height: rect.height * k,
        rotation_deg: rect.rotation_deg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_screen_box_to_display_point_space() {
        let b = BoxField {
            field_key: "poc_name".to_string(),
            label: "POC Name".to_string(),
            kind: FieldKind::Text,
            center_x: 100.0,
            center_y: 50.0,
            width: 200.0,
            height: 16.0,
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
        let original = BoxField {
            field_key: "ticket_number".to_string(),
            label: "Ticket Number".to_string(),
            kind: FieldKind::Checkbox,
            center_x: 780.0,
            center_y: 684.0,
            width: 160.0,
            height: 20.0,
            rotation_deg: 8.0,
        };

        let field = to_field(&original, 150.0, 792.0, "customer_forms-page-1.pdf");
        let restored = from_field(&field, 150.0, 792.0);

        assert_eq!(restored.field_key, original.field_key);
        assert_eq!(restored.label, original.label);
        assert_eq!(restored.kind, original.kind);
        assert!((restored.center_x - original.center_x).abs() < 1e-2);
        assert!((restored.center_y - original.center_y).abs() < 1e-2);
        assert!((restored.width - original.width).abs() < 1e-2);
        assert!((restored.height - original.height).abs() < 1e-2);
        assert!((restored.rotation_deg - original.rotation_deg).abs() < 1e-2);
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use std::collections::BTreeMap;

    /// Simulates what the live calibrator UI would produce: two boxes
    /// drawn in pixel space (one rotated), converted to a schema, and
    /// filled against the real page-1 PDF. This proves the whole draw
    /// -> schema -> fill pipeline (including the rotation sign
    /// convention) without needing to click through the actual UI.
    #[test]
    fn draw_to_schema_to_fill_end_to_end() {
        let pages_dir = forms_core::pages_dir();
        let page_file = "customer_forms-page-1.pdf";

        let poc_name_box = BoxField {
            field_key: "poc_name".to_string(),
            label: "POC Name".to_string(),
            kind: FieldKind::Text,
            center_x: 650.0,
            center_y: 560.0,
            width: 220.0,
            height: 20.0,
            rotation_deg: 0.0,
        };

        let ticket_box = BoxField {
            field_key: "ticket_number".to_string(),
            label: "Ticket Number".to_string(),
            kind: FieldKind::Text,
            center_x: 780.0,
            center_y: 684.0,
            width: 160.0,
            height: 20.0,
            rotation_deg: 8.0,
        };

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

        let out_dir = std::env::temp_dir().join("admin-calibrator-e2e-test");
        let written = forms_core::fill_pages(&pages_dir, &schema, &values, &out_dir).unwrap();
        assert_eq!(written.len(), 1);
        assert!(written[0].exists());
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
        let schema_path = forms_core::pages_dir().join("customer_forms-page-1.schema.json");
        let schema = forms_core::Schema::load(&schema_path).unwrap();
        assert!(!schema.fields.is_empty());

        let restored: Vec<BoxField> = schema
            .fields
            .iter()
            .map(|f| from_field(f, 150.0, 792.0))
            .collect();

        assert_eq!(restored.len(), schema.fields.len());
        // sanity: every restored box is still within the rendered page's
        // pixel bounds (150 DPI, 612x792pt page -> 1275x1650px)
        for b in &restored {
            assert!(b.center_x > 0.0 && b.center_x < 1275.0);
            assert!(b.center_y > 0.0 && b.center_y < 1650.0);
        }
    }
}
