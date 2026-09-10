use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;

use forms_core::{Field, FieldKind, Schema};

use crate::domain::storage;

/// The document/page this option's form fields come from. Hardcoded for
/// now since there's only one form wired up; a second option would get
/// its own constant and its own page the same way.
pub const ISSUE_RECEIPT_SCHEMA_FILE: &str = "customer_forms-page-1.schema.json";

/// Marks the selected option on fields that are really printed yes/no
/// or circle-one choices rather than free text (e.g. "Yes / No",
/// "Reimage / Add to Domain / ..."): forms_core renders this exact
/// value as a thick struck-through line rather than literal text.
pub const CROSSED_OUT: &str = forms_core::STRIKETHROUGH;

/// (field_key, display label) for the "select all that apply" checklist.
pub const APPLY_OPTIONS: [(&str, &str); 5] = [
    ("install_vpn", "Install VPN"),
    ("reimage", "Re-image"),
    ("unlock_local_acount", "Unlock Local Account"),
    ("add_to_domain", "Add to Domain"),
    ("enable_wifi", "Enable Wifi"),
];

/// Every field_key on the Issue Receipt schema that gets a
/// hand-authored row in the client-side form, so the fallback list
/// returned by [`extra_fields`] only ever contains fields nobody's
/// written specific handling for yet.
pub const KNOWN_ISSUE_RECEIPT_KEYS: &[&str] = &[
    "issued_to_signature",
    "Issued_to_name_grade_orgn",
    "serial_number_1",
    "serial_number_2",
    "serial_number_3",
    "serial_number_4",
    "serial_number_5",
    "discription_of_item_1",
    "discription_of_item_2",
    "discription_of_item_3",
    "discription_of_item_4",
    "discription_of_item_5",
    "date_of_issue",
    "POC_name",
    "POC_unit_org",
    "POC_contact_number",
    "ticket_number",
    "yes",
    "no",
    "install_vpn",
    "reimage",
    "unlock_local_acount",
    "add_to_domain",
    "enable_wifi",
    "remarks",
    "customer_intial",
];

/// A field the calibrator produced that the hand-authored form doesn't
/// have a dedicated row for yet — rendered generically on the client
/// under "Other fields" instead of silently vanishing.
#[derive(Debug, Clone, Serialize)]
pub struct ExtraField {
    pub field_key: String,
    pub label: String,
    pub kind: FieldKind,
}

pub fn schema_path() -> PathBuf {
    forms_core::pages_dir().join(ISSUE_RECEIPT_SCHEMA_FILE)
}

/// Loads and dedups the Issue Receipt schema's fields.
pub fn load_fields() -> Result<Vec<Field>> {
    let schema = Schema::load(&schema_path())?;
    Ok(dedup_by_field_key(schema.fields))
}

/// Fields from `fields` that aren't covered by a hand-authored row on
/// the client, in fallback-display form.
pub fn extra_fields(fields: &[Field]) -> Vec<ExtraField> {
    fields
        .iter()
        .filter(|f| !KNOWN_ISSUE_RECEIPT_KEYS.contains(&f.field_key.as_str()))
        .map(|f| ExtraField {
            field_key: f.field_key.clone(),
            label: if f.label.trim().is_empty() {
                prettify_key(&f.field_key)
            } else {
                f.label.clone()
            },
            kind: f.kind,
        })
        .collect()
}

/// Stamps `answers` onto a real copy of the blank template PDF, using
/// the exact same schema and fill logic `doc-calibrator` (now the
/// `/admin/calibrator` tool) verified against — this is the actual
/// finished document, standing in for a print job until there's a
/// printer to send it to. Written to its own subfolder under
/// `completed_forms/`, named with `session_token` so it's easy to
/// match back to the sign-in that produced it.
pub fn fill_completed_pdf(session_token: &str, answers: &BTreeMap<String, String>) -> Result<PathBuf> {
    let schema = Schema::load(&schema_path())?;

    let out_dir = storage::completed_forms_dir().join(format!("temporary_issue_receipt-{session_token}"));
    let written = forms_core::fill_pages(&forms_core::pages_dir(), &schema, answers, &out_dir)?;
    if written.is_empty() {
        anyhow::bail!("schema has no pages to fill");
    }

    Ok(out_dir)
}

/// Keeps only the first `Field` seen per `field_key`: a schema can have
/// the same `field_key` on multiple boxes (even across pages, once more
/// than one page feeds this form) so one customer answer fills every
/// matching box — but that should only produce *one* input on screen.
fn dedup_by_field_key(fields: Vec<Field>) -> Vec<Field> {
    let mut seen = HashSet::new();
    fields
        .into_iter()
        .filter(|f| seen.insert(f.field_key.clone()))
        .collect()
}

/// Falls back to a readable label when a field was never given one in
/// the calibrator: `serial_number_1` -> `Serial Number 1`. Acronym-style
/// words that are already all-uppercase (e.g. `POC`) are left alone.
fn prettify_key(key: &str) -> String {
    key.split('_')
        .map(|word| {
            if word.len() > 1 && word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
                word.to_string()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prettify_key_handles_snake_case_and_acronyms() {
        assert_eq!(prettify_key("serial_number_1"), "Serial Number 1");
        assert_eq!(prettify_key("POC_name"), "POC Name");
        assert_eq!(prettify_key("ticket_number"), "Ticket Number");
    }

    #[test]
    fn dedup_keeps_first_occurrence_per_field_key() {
        let make = |id: &str, label: &str| Field {
            id: id.to_string(),
            field_key: "name".to_string(),
            label: label.to_string(),
            page_file: "p.pdf".to_string(),
            kind: FieldKind::Text,
            rect: forms_core::RotatedRect {
                cx: 0.0,
                cy: 0.0,
                width: 1.0,
                height: 1.0,
                rotation_deg: 0.0,
            },
        };
        let deduped = dedup_by_field_key(vec![make("a", "First"), make("b", "Second")]);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].label, "First");
    }

    #[test]
    fn loads_real_issue_receipt_schema() {
        let fields = load_fields().unwrap();
        assert!(!fields.is_empty());
        for f in &fields {
            assert!(!f.field_key.is_empty());
        }
    }

    /// Every field_key actually in the real schema should be accounted
    /// for by a hand-authored row, so nothing silently falls through to
    /// the generic "Other fields" fallback unless it's genuinely new.
    #[test]
    fn known_keys_cover_the_real_schema() {
        let schema = Schema::load(&schema_path()).unwrap();
        for field in &schema.fields {
            assert!(
                KNOWN_ISSUE_RECEIPT_KEYS.contains(&field.field_key.as_str()),
                "field_key '{}' isn't in KNOWN_ISSUE_RECEIPT_KEYS \
                 (fine if you just calibrated something new — either add \
                 a dedicated row for it or accept it showing up under \
                 'Other fields')",
                field.field_key
            );
        }
    }

    #[test]
    fn loads_placeholder_issuers() {
        let issuers = crate::domain::issuers::load_issuers().unwrap();
        assert!(!issuers.is_empty());
    }

    /// Simulates a customer actually filling out and submitting the
    /// form (populating `answers` the same way the dynamic client-side
    /// form would), then runs the real `fill_completed_pdf` path used
    /// by the submit endpoint, proving the whole pipeline produces an
    /// actual filled PDF from realistic input.
    #[test]
    fn fills_a_realistic_submission_end_to_end() {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        let mut answers = BTreeMap::new();
        for (key, value) in [
            ("Issued_to_name_grade_orgn", "PLACEHOLDER - SSgt Jane Doe, 375th CS"),
            ("issued_to_signature", "PLACEHOLDER Jane Doe"),
            ("serial_number_1", "SN-00123"),
            ("discription_of_item_1", "Dell Latitude 5420 Laptop"),
            ("date_of_issue", &today),
            ("POC_name", "A1C John Public"),
            ("POC_unit_org", "375th MSG"),
            ("POC_contact_number", "555-0100"),
            ("ticket_number", "TCK-4521"),
            ("yes", CROSSED_OUT),
            ("no", ""),
            ("install_vpn", CROSSED_OUT),
            ("reimage", ""),
            ("unlock_local_acount", CROSSED_OUT),
            ("add_to_domain", ""),
            ("enable_wifi", CROSSED_OUT),
            ("remarks", "Customer requested expedited return"),
            ("customer_intial", "JP"),
        ] {
            answers.insert(key.to_string(), value.to_string());
        }

        let out_dir = fill_completed_pdf("test-session-999000111", &answers).unwrap();
        let filled_page = out_dir.join("customer_forms-page-1.pdf");
        assert!(filled_page.exists());

        std::fs::remove_dir_all(&out_dir).unwrap();
    }
}
