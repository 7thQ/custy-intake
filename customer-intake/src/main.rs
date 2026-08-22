mod issuers;
mod storage;

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use forms_core::{Field, FieldKind, Schema};
use issuers::Issuer;

/// The document/page this option's form fields come from. Hardcoded for
/// now since there's only one form wired up; a second option would get
/// its own constant and its own screen the same way.
const ISSUE_RECEIPT_SCHEMA_FILE: &str = "customer_forms-page-1.schema.json";

/// Marks the selected option on fields that are really printed yes/no
/// or circle-one choices rather than free text (e.g. "Yes / No",
/// "Reimage / Add to Domain / ..."): forms_core renders this exact
/// value as a thick struck-through line rather than literal text.
const CROSSED_OUT: &str = forms_core::STRIKETHROUGH;

/// (field_key, display label) for the "select all that apply" checklist.
const APPLY_OPTIONS: [(&str, &str); 5] = [
    ("install_vpn", "Install VPN"),
    ("reimage", "Re-image"),
    ("unlock_local_acount", "Unlock Local Account"),
    ("add_to_domain", "Add to Domain"),
    ("enable_wifi", "Enable Wifi"),
];

/// Every field_key on the Issue Receipt schema that gets a
/// hand-authored row below, so the fallback loop at the bottom only
/// ever renders fields nobody's written specific handling for yet.
const KNOWN_ISSUE_RECEIPT_KEYS: &[&str] = &[
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

#[derive(Clone, PartialEq)]
enum Screen {
    Home,
    IssueReceipt,
}

struct IntakeApp {
    screen: Screen,
    issue_receipt_fields: Vec<Field>,
    schema_error: Option<String>,
    answers: BTreeMap<String, String>,
    status: String,

    issuers: Vec<Issuer>,
    issuers_error: Option<String>,
    selected_issuer: Option<usize>,

    item_count: u32,
    backed_up: Option<bool>,
    apply_selected: [bool; 5],
}

impl Default for IntakeApp {
    fn default() -> Self {
        Self {
            screen: Screen::Home,
            issue_receipt_fields: Vec::new(),
            schema_error: None,
            answers: BTreeMap::new(),
            status: String::new(),
            issuers: Vec::new(),
            issuers_error: None,
            selected_issuer: None,
            item_count: 1,
            backed_up: None,
            apply_selected: [false; 5],
        }
    }
}

impl IntakeApp {
    fn open_issue_receipt(&mut self) {
        let schema_path = forms_core::pages_dir().join(ISSUE_RECEIPT_SCHEMA_FILE);
        match Schema::load(&schema_path) {
            Ok(schema) => {
                self.issue_receipt_fields = dedup_by_field_key(schema.fields);
                self.schema_error = None;
            }
            Err(err) => {
                self.issue_receipt_fields = Vec::new();
                self.schema_error = Some(format!(
                    "Failed to load form fields from {}: {err:#}",
                    schema_path.display()
                ));
            }
        }

        match issuers::load_issuers() {
            Ok(list) => {
                self.issuers = list;
                self.issuers_error = None;
            }
            Err(err) => {
                self.issuers = Vec::new();
                self.issuers_error = Some(format!("Failed to load issuer list: {err:#}"));
            }
        }

        self.answers.clear();
        self.selected_issuer = None;
        self.item_count = 1;
        self.backed_up = None;
        self.apply_selected = [false; 5];

        self.answers.insert(
            "date_of_issue".to_string(),
            chrono::Local::now().format("%Y-%m-%d").to_string(),
        );

        self.screen = Screen::IssueReceipt;
    }

    /// Stamps the current answers onto a real copy of the blank
    /// template PDF, using the exact same schema and fill logic
    /// `doc-calibrator` verified against — this is the actual finished
    /// document, standing in for a print job until there's a printer to
    /// send it to. Written to its own subfolder under
    /// `completed_forms/`, named with `id` so it's easy to match back
    /// to the submission JSON that shares the same id.
    fn fill_completed_pdf(&self, id: i64) -> anyhow::Result<PathBuf> {
        let schema_path = forms_core::pages_dir().join(ISSUE_RECEIPT_SCHEMA_FILE);
        let schema = Schema::load(&schema_path)?;

        let out_dir = storage::completed_forms_dir().join(format!("temporary_issue_receipt-{id}"));
        let written =
            forms_core::fill_pages(&forms_core::pages_dir(), &schema, &self.answers, &out_dir)?;
        if written.is_empty() {
            anyhow::bail!("schema has no pages to fill");
        }

        Ok(out_dir)
    }

    fn show_home(&mut self, ui: &mut egui::Ui) {
        ui.heading("Hello, Welcome to CST Shop");
        ui.label("Select below what you're looking for:");
        ui.add_space(12.0);

        if ui.button("Temporary Issue Receipt (device intake)").clicked() {
            self.open_issue_receipt();
        }

        if !self.status.is_empty() {
            ui.add_space(12.0);
            ui.label(&self.status);
        }
    }

    fn show_issue_receipt(&mut self, ui: &mut egui::Ui) {
        ui.heading("Temporary Issue Receipt");
        ui.separator();

        if ui.button("< Back").clicked() {
            self.screen = Screen::Home;
            return;
        }

        ui.add_space(8.0);

        if let Some(err) = &self.schema_error {
            ui.colored_label(egui::Color32::RED, err);
            return;
        }

        // Pinned above the scroll area: an unpinned Submit button would
        // scroll out of reach the same way the calibrator's field panel
        // once did.
        if ui.button("Submit").clicked() {
            let id = storage::new_submission_id();
            match storage::save_submission(id, "temporary_issue_receipt", self.answers.clone()) {
                Ok(json_path) => {
                    // The submission itself is what matters most and is
                    // saved above; filling the PDF is best-effort on top
                    // of that. Either way the ticket is captured, so we
                    // still return to Home for the next customer even if
                    // this fails — the error just surfaces in status.
                    self.status = match self.fill_completed_pdf(id) {
                        Ok(pdf_dir) => format!(
                            "Saved ticket ({}) and filled form in {}",
                            json_path.display(),
                            pdf_dir.display()
                        ),
                        Err(err) => format!(
                            "Saved ticket ({}) but failed to fill the PDF: {err:#}",
                            json_path.display()
                        ),
                    };
                    self.answers.clear();
                    self.screen = Screen::Home;
                    return;
                }
                Err(err) => {
                    self.status = format!("Failed to save ticket: {err:#}");
                }
            }
        }
        if !self.status.is_empty() {
            ui.add_space(4.0);
            ui.label(&self.status);
        }
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt("issue_receipt_fields_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.show_issue_receipt_fields(ui);
            });
    }

    fn show_issue_receipt_fields(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("issue_receipt_dynamic_form")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                self.show_issuer_row(ui);
                self.show_item_rows(ui);

                ui.label("Date of Issue:");
                ui.label(self.answers.get("date_of_issue").cloned().unwrap_or_default());
                ui.end_row();

                self.show_text_row(ui, "POC_name", "POC Name");
                self.show_text_row(ui, "POC_unit_org", "POC Unit/Org");
                self.show_text_row(ui, "POC_contact_number", "POC Contact Number");
                self.show_text_row(ui, "ticket_number", "Ticket Number");

                self.show_backed_up_row(ui);
                self.show_apply_options_row(ui);

                self.show_text_row(ui, "remarks", "Remarks");
                self.show_text_row(ui, "customer_intial", "Customer Initials");
            });

        // Safety net: any field a future calibration adds that isn't
        // one of the rows above still shows up here, instead of
        // silently vanishing from the form.
        let extra: Vec<Field> = self
            .issue_receipt_fields
            .iter()
            .filter(|f| !KNOWN_ISSUE_RECEIPT_KEYS.contains(&f.field_key.as_str()))
            .cloned()
            .collect();
        if !extra.is_empty() {
            ui.add_space(8.0);
            ui.separator();
            ui.label("Other fields:");
            egui::Grid::new("issue_receipt_extra_fields")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    for field in &extra {
                        let label = if field.label.trim().is_empty() {
                            prettify_key(&field.field_key)
                        } else {
                            field.label.clone()
                        };
                        ui.label(format!("{label}:"));
                        let answer = self.answers.entry(field.field_key.clone()).or_default();
                        match field.kind {
                            FieldKind::Text => {
                                ui.text_edit_singleline(answer);
                            }
                            FieldKind::Checkbox => {
                                let mut checked =
                                    matches!(answer.as_str(), "yes" | "true" | "1");
                                if ui.checkbox(&mut checked, "").changed() {
                                    *answer = if checked { "yes" } else { "no" }.to_string();
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
        }
    }

    fn show_text_row(&mut self, ui: &mut egui::Ui, field_key: &str, label: &str) {
        ui.label(format!("{label}:"));
        let answer = self.answers.entry(field_key.to_string()).or_default();
        ui.text_edit_singleline(answer);
        ui.end_row();
    }

    fn show_issuer_row(&mut self, ui: &mut egui::Ui) {
        ui.label("Issued To:");

        if self.issuers.is_empty() {
            if let Some(err) = &self.issuers_error {
                ui.colored_label(egui::Color32::RED, err);
            } else {
                ui.label("(no issuers configured)");
            }
            ui.end_row();
            return;
        }

        let issuers = self.issuers.clone();
        let current_text = self
            .selected_issuer
            .and_then(|i| issuers.get(i))
            .map(|i| i.name_grade_org.clone())
            .unwrap_or_else(|| "Select who is issuing this...".to_string());

        let mut newly_selected = self.selected_issuer;
        egui::ComboBox::from_id_salt("issuer_select")
            .selected_text(current_text)
            .show_ui(ui, |ui| {
                for (idx, issuer) in issuers.iter().enumerate() {
                    if ui
                        .selectable_label(newly_selected == Some(idx), &issuer.name_grade_org)
                        .clicked()
                    {
                        newly_selected = Some(idx);
                    }
                }
            });
        ui.end_row();

        if newly_selected != self.selected_issuer {
            self.selected_issuer = newly_selected;
            if let Some(idx) = newly_selected {
                let issuer = &issuers[idx];
                self.answers.insert(
                    "Issued_to_name_grade_orgn".to_string(),
                    issuer.name_grade_org.clone(),
                );
                self.answers
                    .insert("issued_to_signature".to_string(), issuer.name.clone());
            }
        }
    }

    fn show_item_rows(&mut self, ui: &mut egui::Ui) {
        ui.label("Number of Items:");
        let mut item_count = self.item_count;
        egui::ComboBox::from_id_salt("item_count_select")
            .selected_text(item_count.to_string())
            .show_ui(ui, |ui| {
                for n in 1..=5u32 {
                    if ui.selectable_label(item_count == n, n.to_string()).clicked() {
                        item_count = n;
                    }
                }
            });
        ui.end_row();

        if item_count != self.item_count {
            // Clear rows no longer shown so a stale value from a
            // previously higher count can't get silently submitted.
            for n in (item_count + 1)..=5 {
                self.answers.remove(&format!("serial_number_{n}"));
                self.answers.remove(&format!("discription_of_item_{n}"));
            }
            self.item_count = item_count;
        }

        for n in 1..=self.item_count {
            ui.label(format!("Serial Number {n}:"));
            let answer = self
                .answers
                .entry(format!("serial_number_{n}"))
                .or_default();
            ui.text_edit_singleline(answer);
            ui.end_row();

            ui.label(format!("Description of Item {n}:"));
            let answer = self
                .answers
                .entry(format!("discription_of_item_{n}"))
                .or_default();
            ui.text_edit_singleline(answer);
            ui.end_row();
        }
    }

    fn show_backed_up_row(&mut self, ui: &mut egui::Ui) {
        ui.label("Was Data Backed Up?:");
        let current_text = match self.backed_up {
            Some(true) => "Yes",
            Some(false) => "No",
            None => "Select...",
        };

        let mut choice = self.backed_up;
        egui::ComboBox::from_id_salt("backed_up_select")
            .selected_text(current_text)
            .show_ui(ui, |ui| {
                if ui.selectable_label(choice == Some(true), "Yes").clicked() {
                    choice = Some(true);
                }
                if ui.selectable_label(choice == Some(false), "No").clicked() {
                    choice = Some(false);
                }
            });
        ui.end_row();

        if choice != self.backed_up {
            self.backed_up = choice;
            match choice {
                Some(true) => {
                    self.answers.insert("yes".to_string(), CROSSED_OUT.to_string());
                    self.answers.insert("no".to_string(), String::new());
                }
                Some(false) => {
                    self.answers.insert("yes".to_string(), String::new());
                    self.answers.insert("no".to_string(), CROSSED_OUT.to_string());
                }
                None => {}
            }
        }
    }

    fn show_apply_options_row(&mut self, ui: &mut egui::Ui) {
        ui.label("Select All That Apply:");

        let selected_count = self.apply_selected.iter().filter(|&&b| b).count();
        let summary = if selected_count == 0 {
            "None selected".to_string()
        } else {
            APPLY_OPTIONS
                .iter()
                .zip(self.apply_selected.iter())
                .filter(|&(_, &sel)| sel)
                .map(|((_, label), _)| *label)
                .collect::<Vec<_>>()
                .join(", ")
        };

        let mut selected = self.apply_selected;
        egui::ComboBox::from_id_salt("apply_options_select")
            .selected_text(summary)
            .show_ui(ui, |ui| {
                for (i, (_, label)) in APPLY_OPTIONS.iter().enumerate() {
                    ui.checkbox(&mut selected[i], *label);
                }
            });
        ui.end_row();

        if selected != self.apply_selected {
            self.apply_selected = selected;
            for (i, (field_key, _)) in APPLY_OPTIONS.iter().enumerate() {
                let value = if selected[i] { CROSSED_OUT } else { "" };
                self.answers.insert((*field_key).to_string(), value.to_string());
            }
        }
    }
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

impl eframe::App for IntakeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.screen.clone() {
            Screen::Home => self.show_home(ui),
            Screen::IssueReceipt => self.show_issue_receipt(ui),
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "CST Customer Intake",
        options,
        Box::new(|_cc| Ok(Box::new(IntakeApp::default()))),
    )
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
        let schema_path = forms_core::pages_dir().join(ISSUE_RECEIPT_SCHEMA_FILE);
        let schema = Schema::load(&schema_path).unwrap();
        let fields = dedup_by_field_key(schema.fields);
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
        let schema_path = forms_core::pages_dir().join(ISSUE_RECEIPT_SCHEMA_FILE);
        let schema = Schema::load(&schema_path).unwrap();
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
        let issuers = issuers::load_issuers().unwrap();
        assert!(!issuers.is_empty());
    }

    /// Simulates a customer actually filling out and submitting the
    /// form (populating `answers` the same way the dynamic-form UI
    /// would), then runs the real `fill_completed_pdf` path used by the
    /// Submit button, proving the whole pipeline produces an actual
    /// filled PDF from realistic input.
    #[test]
    fn fills_a_realistic_submission_end_to_end() {
        let mut app = IntakeApp::default();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

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
            app.answers.insert(key.to_string(), value.to_string());
        }

        let id = 999_000_111;
        let out_dir = app.fill_completed_pdf(id).unwrap();
        let filled_page = out_dir.join("customer_forms-page-1.pdf");
        assert!(filled_page.exists());

        std::fs::remove_dir_all(&out_dir).unwrap();
    }
}
