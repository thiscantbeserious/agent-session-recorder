//! Local sync index `<storage>/.agr-sync.json`, last synced hash and remote id per session.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub(super) const INDEX_FILE: &str = ".agr-sync.json";
pub const LOCK_FILE: &str = ".agr-sync.lock";

/// One synced session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub sha256: String,
    pub remote_id: String,
    pub synced_at: String,
    /// Generation of the last synced manifest, 0 when unknown (old index).
    #[serde(default)]
    pub gen: u64,
    /// Sealed manifest token of the last sync, the `expected` of the next push.
    #[serde(default)]
    pub token: Option<String>,
    /// The gist was created but its first push is not recorded yet, the next run resumes it.
    #[serde(default)]
    pub pending: bool,
}

/// Index keyed by `agent/name.cast.gz`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub entries: BTreeMap<String, IndexEntry>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            version: 1,
            entries: BTreeMap::new(),
        }
    }
}

/// Index file path for a storage directory.
pub fn path_for(storage_dir: &Path) -> PathBuf {
    storage_dir.join(INDEX_FILE)
}

/// Load the index, missing is empty, unparseable is empty plus a warning.
pub fn load(storage_dir: &Path) -> (Index, Option<String>) {
    let path = path_for(storage_dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (Index::default(), None);
    };
    match serde_json::from_str(&text) {
        Ok(index) => (index, None),
        Err(_) => (
            Index::default(),
            Some(format!("{} is not valid, rebuilding it", path.display())),
        ),
    }
}

/// Save atomically.
pub(super) fn save(storage_dir: &Path, index: &Index) -> Result<()> {
    let json = serde_json::to_string_pretty(index)?;
    crate::files::codec::atomic_write(
        &path_for(storage_dir),
        |w| Ok(w.write_all(json.as_bytes())?),
    )
}

#[cfg(test)]
mod tests {
    use super::{load, path_for, save, Index, IndexEntry, Path, PathBuf, INDEX_FILE, LOCK_FILE};

    fn entry(n: u8) -> IndexEntry {
        IndexEntry {
            sha256: format!("hash{n}"),
            remote_id: format!("id{n}"),
            synced_at: "2026-11-06T10:00:00+00:00".into(),
            ..IndexEntry::default()
        }
    }

    #[test]
    fn index_lives_in_hidden_file_at_storage_root() {
        assert_eq!(
            path_for(Path::new("/s")),
            PathBuf::from("/s/.agr-sync.json")
        );
    }

    #[test]
    fn missing_index_is_empty_without_warning() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()), (Index::default(), None));
    }

    #[test]
    fn entries_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = Index::default();
        index.entries.insert("claude/a.cast.gz".into(), entry(1));
        index.entries.insert("codex/b.cast.gz".into(), entry(2));
        save(dir.path(), &index).unwrap();
        assert_eq!(load(dir.path()), (index, None));
    }

    #[test]
    fn unparseable_index_loads_empty_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(path_for(dir.path()), "{not json").unwrap();
        let (index, warning) = load(dir.path());
        assert_eq!(index, Index::default());
        assert!(warning.is_some_and(|w| w.contains(".agr-sync.json")));
    }

    #[test]
    fn save_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &Index::default()).unwrap();
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![INDEX_FILE.to_string()]);
    }

    #[test]
    fn index_and_lock_files_are_not_sessions_or_stats() {
        let dir = tempfile::tempdir().unwrap();
        let agent = dir.path().join("claude");
        std::fs::create_dir_all(&agent).unwrap();
        std::fs::write(agent.join("a.cast"), "{}\n").unwrap();
        save(dir.path(), &Index::default()).unwrap();
        std::fs::write(dir.path().join(LOCK_FILE), "{}").unwrap();
        let mut config = crate::Config::default();
        config.storage.directory = dir.path().to_string_lossy().to_string();
        let storage = crate::StorageManager::new(config);
        let sessions = storage.list_sessions(None).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(storage.get_stats().unwrap().session_count, 1);
    }
}
