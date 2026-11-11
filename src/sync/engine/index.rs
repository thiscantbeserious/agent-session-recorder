//! Local sync index `<storage>/.agr-sync.json`, last synced hash and gen per session.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

pub(super) const INDEX_FILE: &str = ".agr-sync.json";
pub const LOCK_FILE: &str = ".agr-sync.lock";

/// One synced session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub sha256: String,
    pub synced_at: String,
    /// Generation of the last synced manifest, 0 when unknown (old index).
    #[serde(default)]
    pub gen: u64,
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
/// Old entries without a stamp are listed too (hashed once, the plan finds them unchanged).
pub fn maybe_changed(storage_dir: &Path, index: &Index) -> Vec<String> {
    let differs = |(path, e): &(&String, &IndexEntry)| {
        let now = stamp(&storage_dir.join(path));
        now != (0, 0) && now != (e.size, e.mtime_ns)
    };
    let all = index.entries.iter().filter(differs);
    all.map(|(path, _)| path.clone()).collect()
}

/// Index keyed by `agent/name.cast.gz`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub entries: BTreeMap<String, IndexEntry>,
    /// Id of the open pack of this machine, the hint of the next `Repo::open`.
    #[serde(default)]
    pub open_pack: Option<String>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            version: 1,
            entries: BTreeMap::new(),
            open_pack: None,
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

/// Why the pack engine will not plan against the local index file.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refused {
    /// Written by the per-session engine, `agr sync migrate` converts it.
    V1,
    /// Exists but does not parse, treating it as empty would plan every local edit as new and
    /// the next save would overwrite it.
    Corrupt(String),
}

/// Like `load` for the pack engine: a missing file is an empty v2 index, a v1 file or a file
/// that does not parse is refused so nothing is planned against it.
pub(super) fn load_v2(storage_dir: &Path) -> Result<Index, Refused> {
    let (mut index, warning) = load(storage_dir);
    if warning.is_some() {
        let path = path_for(storage_dir);
        let text = format!("{} is not valid, fix or remove it first", path.display());
        return Err(Refused::Corrupt(text));
    }
    if path_for(storage_dir).exists() && index.version < 2 {
        return Err(Refused::V1);
    }
    index.version = 2;
    Ok(index)
}

/// The entry of a session the packs hold at `gen` with `sha256`, `stamp` is the one taken before
/// the bytes behind `sha256` were read.
pub(super) fn record_synced(
    index: &mut Index,
    path: &str,
    (sha256, gen): (&str, u64),
    (size, mtime_ns): (u64, u64),
) {
    let entry = IndexEntry {
        sha256: sha256.to_string(),
        synced_at: chrono::Utc::now().to_rfc3339(),
        gen,
        size,
        mtime_ns,
    };
    index.entries.insert(path.to_string(), entry);
}

/// Save atomically.
pub(super) fn save(storage_dir: &Path, index: &Index) -> Result<()> {
    let json = serde_json::to_string_pretty(index)?;
    crate::files::codec::atomic_write(
        &path_for(storage_dir),
        |w| Ok(w.write_all(json.as_bytes())?),
    )
}

/// What v2 keeps of a v1 entry: hash and gen so the plan keeps its last synced state, the
/// stamp so an unchanged file is not hashed again. The old remote id, token and pending flag are gone.
fn carry(path: &str, e: &Value) -> Result<Value, String> {
    let text = |key: &str| {
        e[key]
            .as_str()
            .ok_or_else(|| format!("entry {path} has no {key}"))
    };
    Ok(json!({
        "sha256": text("sha256")?,
        "synced_at": text("synced_at")?,
        "gen": e["gen"].as_u64().unwrap_or(0),
        "size": e["size"].as_u64().unwrap_or(0),
        "mtime_ns": e["mtime_ns"].as_u64().unwrap_or(0),
    }))
}

/// The v2 text of a v1 index, `None` when it already is v2. A pending entry never synced
/// anything, so it is dropped and its local file reads as new.
pub fn convert_index(text: &str) -> Result<Option<String>, String> {
    let old: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if old["version"].as_u64().unwrap_or(1) >= 2 {
        return Ok(None);
    }
    let old_entries = old["entries"]
        .as_object()
        .ok_or("entries is not an object")?;
    let entries = (old_entries.iter())
        .filter(|(_, e)| e["pending"] != true)
        .map(|(path, e)| Ok((path.clone(), carry(path, e)?)))
        .collect::<Result<Map<String, Value>, String>>()?;
    let v2 = json!({ "version": 2, "entries": entries, "open_pack": null });
    Ok(Some(v2.to_string()))
}

/// Converts `<dir>/.agr-sync.json` in place when it is v1, true when it was written.
/// `agr sync migrate` gets this as a closure, the pack layer never names the index file.
/// A missing file is left alone, the engine starts an empty v2 from it. A file that exists
/// but cannot be read or parsed aborts, the engine would rebuild it empty and plan unsynced
/// local edits as new.
pub fn convert_file(dir: &Path) -> Result<bool, String> {
    let path = path_for(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("could not read {}: {e}", path.display())),
    };
    let converted = convert_index(&text).map_err(|e| {
        format!(
            "{} is not valid ({e}), fix or remove it first",
            path.display()
        )
    })?;
    let Some(v2) = converted else {
        return Ok(false);
    };
    crate::files::codec::atomic_write(&path, |w| Ok(w.write_all(v2.as_bytes())?))
        .map_err(|e| format!("could not write {}: {e}", path.display()))?;
    Ok(true)
}

// inline: save and INDEX_FILE are private to the engine, the tests write and read the index file directly
#[cfg(test)]
mod tests {
    use super::{load, path_for, save, Index, IndexEntry, Path, PathBuf, INDEX_FILE, LOCK_FILE};

    fn entry(n: u8) -> IndexEntry {
        IndexEntry {
            sha256: format!("hash{n}"),
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
