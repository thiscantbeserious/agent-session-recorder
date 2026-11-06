//! Index entry written after a push, refresh or pull.

use super::fail::msg;
use super::index::{self, Index, IndexEntry};
use crate::StorageManager;

/// What the last sync knew about one remote copy.
pub(super) struct Synced {
    pub sha256: String,
    pub remote_id: String,
    pub gen: u64,
    pub token: String,
}

pub(super) fn record(index: &mut Index, path: &str, synced: Synced) {
    let entry = IndexEntry {
        sha256: synced.sha256,
        remote_id: synced.remote_id,
        synced_at: chrono::Utc::now().to_rfc3339(),
        gen: synced.gen,
        token: Some(synced.token),
        pending: false,
    };
    index.entries.insert(path.to_string(), entry);
}

/// A gist that was created and not pushed yet, the next run resumes `remote_id`.
pub(super) fn record_pending(index: &mut Index, path: &str, remote_id: &str) {
    let entry = IndexEntry {
        remote_id: remote_id.to_string(),
        synced_at: chrono::Utc::now().to_rfc3339(),
        pending: true,
        ..IndexEntry::default()
    };
    index.entries.insert(path.to_string(), entry);
}

/// The one place an engine run writes the index.
pub(super) fn save_index(storage: &StorageManager, index: &Index) -> Result<(), String> {
    index::save(&storage.storage_dir(), index).map_err(msg)
}
