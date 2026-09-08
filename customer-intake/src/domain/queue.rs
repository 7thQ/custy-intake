use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// CST, Neets, and LRA each run the exact same "customers waiting"
/// queue — this identifies which one a given call is about, so the
/// storage and list/checkin/remove logic below is written once and
/// shared across all three instead of tripled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueId {
    Cst,
    Neets,
    Lra,
}

impl QueueId {
    pub fn from_portal_id(id: &str) -> Option<Self> {
        match id {
            "cst" => Some(QueueId::Cst),
            "neets" => Some(QueueId::Neets),
            "lra" => Some(QueueId::Lra),
            _ => None,
        }
    }

    fn file_name(self) -> &'static str {
        match self {
            QueueId::Cst => "cst.json",
            QueueId::Neets => "neets.json",
            QueueId::Lra => "lra.json",
        }
    }
}

/// `id` is a nanosecond timestamp, which overflows JS's safe-integer
/// range (2^53) — serialized as a bare JSON number it gets silently
/// rounded by any browser that reads it back, so a "Remove" click
/// would send a slightly different id than the one actually stored
/// and 404. Round-tripping it as a string keeps full precision.
fn serialize_id<S: Serializer>(id: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&id.to_string())
}

fn deserialize_id<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
}

/// One customer waiting in a queue. `name` is a single free-text field
/// on purpose — it's either "Rank Name" (e.g. "SSgt Jane Doe") or just
/// a plain name for a local national or contractor; that distinction
/// lives in what gets typed, not in the schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEntry {
    #[serde(serialize_with = "serialize_id", deserialize_with = "deserialize_id")]
    pub id: i64,
    pub name: String,
    pub description: String,
    pub checked_in_at: String,
}

/// Where every queue's own `<queue>.json` lives.
pub fn queues_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/queues"))
}

fn queue_path_in(dir: &Path, queue: QueueId) -> PathBuf {
    dir.join(queue.file_name())
}

fn load_in(dir: &Path, queue: QueueId) -> Result<Vec<QueueEntry>> {
    let path = queue_path_in(dir, queue);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn save_in(dir: &Path, queue: QueueId, entries: &[QueueEntry]) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let path = queue_path_in(dir, queue);
    fs::write(&path, serde_json::to_vec_pretty(entries)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Oldest-first, so the list is already in queue order.
pub fn list_in(dir: &Path, queue: QueueId) -> Result<Vec<QueueEntry>> {
    let mut entries = load_in(dir, queue)?;
    entries.sort_by_key(|e| e.id);
    Ok(entries)
}

pub fn checkin_in(dir: &Path, queue: QueueId, name: String, description: String) -> Result<QueueEntry> {
    let mut entries = load_in(dir, queue)?;
    let entry = QueueEntry {
        id: chrono::Local::now().timestamp_nanos_opt().unwrap_or_default(),
        name,
        description,
        checked_in_at: chrono::Local::now().to_rfc3339(),
    };
    entries.push(entry.clone());
    save_in(dir, queue, &entries)?;
    Ok(entry)
}

/// Returns whether an entry was actually found and removed.
pub fn remove_in(dir: &Path, queue: QueueId, entry_id: i64) -> Result<bool> {
    let mut entries = load_in(dir, queue)?;
    let before = entries.len();
    entries.retain(|e| e.id != entry_id);
    let removed = entries.len() != before;
    if removed {
        save_in(dir, queue, &entries)?;
    }
    Ok(removed)
}

pub fn list(queue: QueueId) -> Result<Vec<QueueEntry>> {
    list_in(&queues_dir(), queue)
}

pub fn checkin(queue: QueueId, name: String, description: String) -> Result<QueueEntry> {
    checkin_in(&queues_dir(), queue, name, description)
}

pub fn remove(queue: QueueId, entry_id: i64) -> Result<bool> {
    remove_in(&queues_dir(), queue, entry_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn from_portal_id_maps_known_ids_only() {
        assert_eq!(QueueId::from_portal_id("cst"), Some(QueueId::Cst));
        assert_eq!(QueueId::from_portal_id("neets"), Some(QueueId::Neets));
        assert_eq!(QueueId::from_portal_id("lra"), Some(QueueId::Lra));
        assert_eq!(QueueId::from_portal_id("admin"), None);
        assert_eq!(QueueId::from_portal_id("bogus"), None);
    }

    #[test]
    fn checkin_list_remove_round_trip() {
        let dir = temp_dir("queue-test-round-trip");

        assert!(list_in(&dir, QueueId::Cst).unwrap().is_empty());

        let entry = checkin_in(&dir, QueueId::Cst, "SSgt Jane Doe".to_string(), "Laptop won't boot".to_string())
            .unwrap();
        let listed = list_in(&dir, QueueId::Cst).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, entry.id);
        assert_eq!(listed[0].name, "SSgt Jane Doe");

        let removed = remove_in(&dir, QueueId::Cst, entry.id).unwrap();
        assert!(removed);
        assert!(list_in(&dir, QueueId::Cst).unwrap().is_empty());

        let removed_again = remove_in(&dir, QueueId::Cst, entry.id).unwrap();
        assert!(!removed_again);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn list_is_oldest_first() {
        let dir = temp_dir("queue-test-order");

        let first = checkin_in(&dir, QueueId::Neets, "A".to_string(), "first".to_string()).unwrap();
        let second = checkin_in(&dir, QueueId::Neets, "B".to_string(), "second".to_string()).unwrap();

        let listed = list_in(&dir, QueueId::Neets).unwrap();
        assert_eq!(listed.iter().map(|e| e.id).collect::<Vec<_>>(), vec![first.id, second.id]);

        fs::remove_dir_all(&dir).unwrap();
    }

    /// Queues are independent files: checking someone into CST must not
    /// show up in Neets' or LRA's list.
    #[test]
    fn queues_do_not_bleed_into_each_other() {
        let dir = temp_dir("queue-test-isolation");

        checkin_in(&dir, QueueId::Cst, "Cst Customer".to_string(), "d".to_string()).unwrap();

        assert_eq!(list_in(&dir, QueueId::Cst).unwrap().len(), 1);
        assert!(list_in(&dir, QueueId::Neets).unwrap().is_empty());
        assert!(list_in(&dir, QueueId::Lra).unwrap().is_empty());

        fs::remove_dir_all(&dir).unwrap();
    }
}
