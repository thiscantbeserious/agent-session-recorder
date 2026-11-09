//! Local cache of the sealed index texts, keyed by gist id plus the listing `updated_at`.
//! Sealed text only, written with temp plus rename, a corrupt file is ignored and rebuilt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Entry {
    pub updated_at: String,
    pub index: String,
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

    fn temp_path(&self) -> PathBuf {
        self.path.with_extension("tmp")
    }

    fn save(&self, all: &BTreeMap<String, Entry>) {
        let Ok(text) = serde_json::to_string(all) else {
            return;
        };
        let tmp = self.temp_path();
        let dir = self.path.parent().map(std::fs::create_dir_all);
        if dir.is_some_and(|r| r.is_err()) || std::fs::write(&tmp, text).is_err() {
            return;
        }
        let _ = std::fs::rename(&tmp, &self.path);
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
    use super::{Entry, IndexCache};

    #[test]
    fn cache_temp_names_are_unique() {
        let cache = IndexCache::new(&std::env::temp_dir().join("index-cache.json"));
        assert_ne!(cache.temp_path(), cache.temp_path());
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
