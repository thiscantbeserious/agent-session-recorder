//! Local cache of the sealed index texts, keyed by gist id plus the listing `updated_at`.
//! Sealed text only, written with temp plus rename, a corrupt file is ignored and rebuilt.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Entry {
    pub updated_at: String,
    pub index: String,
}

/// The cache holds secret gist `raw_url`s, so the dir is 0700 and the file 0600.
// Windows: the user profile dir is private by its ACL, no mode to set.
pub(super) fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

pub(super) fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(text.as_bytes())
}

/// A temp name of its own per call (pid plus counter) in the same dir, so writers never share an inode.
pub(super) fn temp_path(path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("{}.{n}.tmp", std::process::id()))
}

/// Writes `text` to a temp file (0600) next to `path`, then renames it over `path`.
pub(super) fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = temp_path(path);
    if let Some(dir) = path.parent() {
        make_private_dir(dir)?;
    }
    write_private(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub(super) struct IndexCache {
    path: PathBuf,
}

impl IndexCache {
    pub(super) fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    fn load(&self) -> BTreeMap<String, Entry> {
        let text = std::fs::read_to_string(&self.path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn save(&self, all: &BTreeMap<String, Entry>) {
        if let Ok(text) = serde_json::to_string(all) {
            let _ = write_atomic(&self.path, &text);
        }
    }

    /// The entry of `id`, only when it was stored for exactly this `updated_at`.
    pub(super) fn get(&self, id: &str, updated_at: &str) -> Option<Entry> {
        self.load()
            .remove(id)
            .filter(|e| e.updated_at == updated_at)
    }

    pub(super) fn put(&self, id: &str, entry: Entry) {
        let mut all = self.load();
        all.insert(id.to_string(), entry);
        self.save(&all);
    }

    /// Forgets `id`, called when an append came back `Moved`.
    #[allow(dead_code)] // `append` of Stage 53 calls it
    pub(super) fn drop_entry(&self, id: &str) {
        let mut all = self.load();
        if all.remove(id).is_some() {
            self.save(&all);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{temp_path, Entry, IndexCache};

    #[test]
    fn cache_temp_names_are_unique() {
        let path = std::env::temp_dir().join("index-cache.json");
        assert_ne!(temp_path(&path), temp_path(&path));
    }

    #[cfg(unix)]
    #[test]
    fn index_cache_file_is_0600_in_a_0700_dir() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let file = base.path().join("agr").join("index-cache.json");
        let entry = Entry {
            updated_at: "t1".into(),
            index: "idx".into(),
        };
        IndexCache::new(&file).put("a", entry);
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode();
        assert_eq!(mode(&file) & 0o777, 0o600);
        assert_eq!(mode(file.parent().unwrap()) & 0o777, 0o700);
    }

    #[test]
    fn cache_entry_dropped_on_moved() {
        let dir = tempfile::tempdir().unwrap();
        let cache = IndexCache::new(&dir.path().join("index-cache.json"));
        let entry = Entry {
            updated_at: "t1".into(),
            index: "idx".into(),
        };
        cache.put("a", entry);
        cache.drop_entry("a");
        assert!(cache.get("a", "t1").is_none());
    }
}
