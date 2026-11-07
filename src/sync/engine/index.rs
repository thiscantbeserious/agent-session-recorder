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
    /// On-disk size and mtime of the local file when it was synced, 0 when unknown (old index).
    /// A cheap "might have changed" check, never an input of the plan, the hash decides.
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mtime_ns: u64,
}

/// On-disk size and mtime in nanoseconds of a file, zeros when it cannot be read.
pub fn stamp(path: &Path) -> (u64, u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .unwrap_or(0);
    (meta.len(), nanos)
}

/// Paths whose file exists and no longer matches the size and mtime of their last sync.
/// Old entries without a stamp are listed too (hashed once, the plan finds them unchanged). Pending entries are left to a manual push.
pub fn maybe_changed(storage_dir: &Path, index: &Index) -> Vec<String> {
    let differs = |(path, e): &(&String, &IndexEntry)| {
        let now = stamp(&storage_dir.join(path));
        !e.pending && now != (0, 0) && now != (e.size, e.mtime_ns)
    };
    let all = index.entries.iter().filter(differs);
    all.map(|(path, _)| path.clone()).collect()
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
    fn old_entries_load_without_a_stamp_and_changed_files_are_found() {
        let json = r#"{"version":1,"entries":{"a/x.cast.gz":{"sha256":"h","remote_id":"i","synced_at":"t"}}}"#;
        let mut index: Index = serde_json::from_str(json).unwrap();
        assert_eq!(
            (
                index.entries["a/x.cast.gz"].size,
                index.entries["a/x.cast.gz"].mtime_ns
            ),
            (0, 0)
        );
        let dir = tempfile::tempdir().unwrap();
        assert!(
            super::maybe_changed(dir.path(), &index).is_empty(),
            "missing file"
        );
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        let file = dir.path().join("a/x.cast.gz");
        std::fs::write(&file, "abc").unwrap();
        assert_eq!(super::maybe_changed(dir.path(), &index), ["a/x.cast.gz"]);
        let (size, mtime_ns) = super::stamp(&file);
        let entry = index.entries.get_mut("a/x.cast.gz").unwrap();
        (entry.size, entry.mtime_ns) = (size, mtime_ns);
        assert!(super::maybe_changed(dir.path(), &index).is_empty());
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
