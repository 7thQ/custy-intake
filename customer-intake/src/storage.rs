use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Serialize)]
struct Submission {
    form: String,
    submitted_at: String,
    fields: BTreeMap<String, String>,
}

/// Where every submitted form's answers are stored. Hardcoded, same
/// pattern as doc-calibrator's `pages/` folder: a plain visible directory,
/// no OS-specific cache location to hunt for.
pub fn submissions_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/submissions"))
}

/// Where the actual filled-out PDFs land — a stand-in for sending a
/// print job, until there's a printer to send to. Each submission gets
/// its own subfolder (named with the same `id` as its JSON record in
/// `submissions_dir()`, so the two are easy to correlate) containing
/// the filled page(s).
pub fn completed_forms_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/completed_forms"))
}

/// A fresh id shared between a submission's JSON record and its filled
/// PDF's output folder, so the two land under matching names.
pub fn new_submission_id() -> i64 {
    chrono::Local::now().timestamp_nanos_opt().unwrap_or_default()
}

/// Saves one filled-out form's answers as a JSON record, keyed by
/// `field_key` names. These are the same keys the document calibrator
/// tags its text boxes with, so a schema's `field_key` and a
/// submission's `fields` key are how the two programs agree on which
/// answer goes in which box.
pub fn save_submission(id: i64, form: &str, fields: BTreeMap<String, String>) -> Result<PathBuf> {
    let dir = submissions_dir();
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let file_name = format!("{form}-{id}.json");
    let path = dir.join(file_name);

    let submission = Submission {
        form: form.to_string(),
        submitted_at: chrono::Local::now().to_rfc3339(),
        fields,
    };

    fs::write(&path, serde_json::to_vec_pretty(&submission)?)
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_a_submission_record() {
        let mut fields = BTreeMap::new();
        fields.insert("serial_number".to_string(), "SN12345".to_string());
        fields.insert("ticket_number".to_string(), "TCK-9001".to_string());

        let path = save_submission(123456789, "temporary_issue_receipt", fields).unwrap();
        assert!(path.exists());

        let contents = fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(value["form"], "temporary_issue_receipt");
        assert_eq!(value["fields"]["serial_number"], "SN12345");

        fs::remove_file(&path).unwrap();
    }
}
