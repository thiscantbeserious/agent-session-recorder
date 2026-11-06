//! Index entry written after a push, refresh or pull.

use super::index::{Index, IndexEntry};

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
