//! Machines for the pack engine tests, one storage dir each, stores are passed in.

use agr::files::codec;
use agr::sync::testing::PackStore;
use agr::sync::{Engine, Key, Options};
use agr::{Config, StorageManager};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub const TARGET: u64 = 64 * 1024 * 1024;
pub const CAST: &str = "{\"version\":3,\"width\":80,\"height\":24}\n[0.1,\"o\",\"secret text\"]\n";
pub const A: &str = "claude/a.cast.gz";

pub struct Node {
    temp: TempDir,
    pub storage: StorageManager,
}

pub fn node() -> Node {
    let temp = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.directory = temp.path().to_string_lossy().to_string();
    Node {
        storage: StorageManager::new(config),
        temp,
    }
}

pub fn opts() -> Options {
    Options::default()
}

pub fn overwrite() -> Options {
    Options {
        overwrite: true,
        ..Options::default()
    }
}

impl Node {
    pub fn dir(&self) -> &Path {
        self.temp.path()
    }

    pub fn session(&self, agent: &str, name: &str, text: &str) -> PathBuf {
        let dir = self.dir().join(agent);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        codec::atomic_write(&path, |w| Ok(w.write_all(text.as_bytes())?)).unwrap();
        path
    }

    pub fn engine<'a>(
        &'a self,
        store: &'a dyn PackStore,
        key: &'a Key,
        writer: &str,
    ) -> Engine<'a> {
        Engine::new(store, key, &self.storage, writer, TARGET)
    }

    pub fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        fs::read(self.dir().join(path)).ok()
    }

    pub fn names(&self, agent: &str) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.dir().join(agent))
            .map(|rd| {
                rd.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

/// Three sessions, two agents.
pub fn three(n: &Node) {
    n.session("claude", "a.cast.gz", CAST);
    n.session("claude", "b.cast.gz", &CAST.replace("secret", "other"));
    n.session("codex", "c.cast.gz", &CAST.replace("secret", "third"));
}

/// Gen and tombstone flag of `path` in the merged view, plus the paths that are forks.
pub fn merged(store: &dyn PackStore, key: &Key, path: &str) -> (Option<(u64, bool)>, Vec<String>) {
    let repo = agr::sync::testing::Repo::open(store, key, "reader", TARGET, None).unwrap();
    let at = repo.view().get(path).map(|m| (m.gen, m.is_deleted()));
    (at, repo.view().forks().to_vec())
}

/// A raw one-session push of `text` at `gen` by `writer`, bypassing the engine.
pub fn raw_push(
    store: &dyn PackStore,
    key: &Key,
    writer: &str,
    (path, gen): (&str, u64),
    text: &str,
) {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("raw");
    fs::write(&file, text).unwrap();
    let item = agr::sync::testing::PushItem {
        path: path.to_string(),
        gen,
        file,
    };
    let mut repo = agr::sync::testing::Repo::open(store, key, writer, TARGET, None).unwrap();
    repo.push_batch(&[item]).unwrap();
}
