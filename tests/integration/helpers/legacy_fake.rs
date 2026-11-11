//! In-memory old per-session gists for the migrate tests, with the counters the data safety asserts need.

use agr::sync::testing::blob::seal_blob;
use agr::sync::testing::{seal_meta, sha256_hex, LegacyGist, LegacyRead, Meta, StoreError};
use agr::sync::Key;
use std::io::{Cursor, Read};
use std::sync::Mutex;

struct Gist {
    info: LegacyGist,
    manifest: Option<String>,
    blob: Vec<u8>,
}

#[derive(Default)]
pub struct LegacyFake {
    gists: Mutex<Vec<Gist>>,
    deletes: Mutex<Vec<String>>,
    /// Deletes allowed before `RateLimited`, `None` is no limit.
    delete_budget: Mutex<Option<usize>>,
    /// Deletes allowed before a hard error, `None` is never.
    delete_fail_after: Mutex<Option<usize>>,
    /// Every `pull` fails as unreachable while set.
    pull_unreachable: Mutex<bool>,
}

pub const OLD: i64 = 5 * 3600;

impl LegacyFake {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&self, info: LegacyGist, manifest: Option<String>, blob: Vec<u8>) -> String {
        let id = info.id.clone();
        self.gists.lock().unwrap().push(Gist {
            info,
            manifest,
            blob,
        });
        id
    }

    fn next_id(&self) -> String {
        format!("g{}", self.gists.lock().unwrap().len() + 1)
    }

    /// A v1 session of `key`, the description claims `claimed_sha` when given.
    pub fn v1(
        &self,
        key: &Key,
        (path, gen): (&str, u64),
        plain: &[u8],
        claimed: Option<&str>,
    ) -> String {
        let sha256 = claimed.map_or_else(|| sha256_hex(plain), String::from);
        let meta = Meta {
            v: 1,
            path: path.into(),
            sha256,
            size: plain.len() as u64,
            gen,
        };
        let token = seal_meta(key, &meta);
        let files = ["manifest", "data.000"].map(|n| (n.to_string(), 9));
        let info = LegacyGist {
            id: self.next_id(),
            description: format!("agr:v1 {token}"),
            files: [(".keep".to_string(), 4)]
                .into_iter()
                .chain(files)
                .collect(),
            created_at: at(OLD),
            truncated: false,
        };
        self.push(info, Some(token), seal_blob(key, path, gen, plain))
    }

    /// A v1 session of `key` with one more file in the gist, nothing agr wrote.
    pub fn v1_with_extra(&self, key: &Key, (path, gen): (&str, u64), plain: &[u8]) -> String {
        let id = self.v1(key, (path, gen), plain, None);
        let mut all = self.gists.lock().unwrap();
        all.last_mut()
            .unwrap()
            .info
            .files
            .push(("notes.txt".into(), 1));
        id
    }

    /// A v1 gist whose part is missing.
    pub fn v1_without_part(&self, key: &Key, path: &str) -> String {
        let id = self.v1(key, (path, 1), b"x", None);
        self.gists.lock().unwrap().last_mut().unwrap().blob.clear();
        id
    }

    pub fn stub(&self, age: i64) -> String {
        let info = LegacyGist {
            id: self.next_id(),
            description: "agr:pending".into(),
            files: vec![(".keep".into(), 4)],
            created_at: at(age),
            truncated: false,
        };
        self.push(info, None, Vec::new())
    }

    /// A gist a crashed push left, the blob and manifest are in, the description still says pending.
    pub fn pending_with_content(&self, key: &Key, path: &str, plain: &[u8]) -> String {
        let id = self.v1(key, (path, 1), plain, None);
        let mut all = self.gists.lock().unwrap();
        all.last_mut().unwrap().info.description = "agr:pending".into();
        id
    }

    /// A gist of somebody else's key.
    pub fn foreign(&self, path: &str) -> String {
        self.v1(&Key::generate(), (path, 1), b"x", None)
    }

    pub fn plain(&self, n: usize) {
        for _ in 0..n {
            let info = LegacyGist {
                id: self.next_id(),
                description: "notes".into(),
                files: vec![("a.txt".into(), 1)],
                created_at: at(OLD),
                truncated: false,
            };
            self.push(info, None, Vec::new());
        }
    }

    pub fn deletes(&self) -> Vec<String> {
        self.deletes.lock().unwrap().clone()
    }

    /// The user's own `gh gist delete`, not counted as a migrate delete.
    pub fn delete_by_hand(&self, id: &str) {
        self.gists.lock().unwrap().retain(|g| g.info.id != id);
    }

    pub fn limit_deletes(&self, n: Option<usize>) {
        *self.delete_budget.lock().unwrap() = n;
    }

    pub fn pull_unreachable(&self, on: bool) {
        *self.pull_unreachable.lock().unwrap() = on;
    }

    pub fn fail_deletes_after(&self, n: usize) {
        *self.delete_fail_after.lock().unwrap() = Some(n);
    }

    pub fn descriptions(&self) -> Vec<String> {
        let all = self.gists.lock().unwrap();
        all.iter().map(|g| g.info.description.clone()).collect()
    }
}

pub fn at(secs_ago: i64) -> String {
    chrono::DateTime::from_timestamp(NOW - secs_ago, 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub const NOW: i64 = 1_800_000_000;

impl LegacyRead for LegacyFake {
    fn list_all(&self) -> Result<Vec<LegacyGist>, StoreError> {
        Ok(self
            .gists
            .lock()
            .unwrap()
            .iter()
            .map(|g| g.info.clone())
            .collect())
    }

    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), StoreError> {
        if *self.pull_unreachable.lock().unwrap() {
            return Err(StoreError::Unreachable("network down".into()));
        }
        let all = self.gists.lock().unwrap();
        let g = all
            .iter()
            .find(|g| g.info.id == id)
            .ok_or(StoreError::NotFound)?;
        Ok((g.manifest.clone(), Box::new(Cursor::new(g.blob.clone()))))
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        if !self.gists.lock().unwrap().iter().any(|g| g.info.id == id) {
            return Err(StoreError::NotFound);
        }
        let mut fail = self.delete_fail_after.lock().unwrap();
        if *fail == Some(0) {
            return Err(StoreError::Other("HTTP 500".into()));
        }
        *fail = fail.map(|n| n - 1);
        let mut budget = self.delete_budget.lock().unwrap();
        if *budget == Some(0) {
            return Err(StoreError::RateLimited);
        }
        *budget = budget.map(|n| n - 1);
        self.deletes.lock().unwrap().push(id.to_string());
        self.gists.lock().unwrap().retain(|g| g.info.id != id);
        Ok(())
    }
}

use agr::sync::testing::{Listing, MemoryStore, MigrateEnv, PackStore, PushItem, Repo, Segment};
use tempfile::TempDir;

pub const TARGET: u64 = 64 * 1024;

/// The pack store, with the listing of old gists read live from the fake, as the gist store shows it.
pub struct Listed<'a>(pub &'a MemoryStore, pub &'a LegacyFake);

impl PackStore for Listed<'_> {
    fn name(&self) -> &'static str {
        "listed"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.0.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        let mut l = self.0.list()?;
        let v1 = |d: &String| d.starts_with("agr:v1 ");
        l.legacy = self.1.descriptions().into_iter().filter(v1).collect();
        Ok(l)
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        self.0.read_index(id)
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.0.create(tag, index)
    }
    fn append(&self, id: &str, n: u64, e: &str, s: &[Segment], i: &str) -> Result<(), StoreError> {
        self.0.append(id, n, e, s, i)
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.0.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.0.delete(id)
    }
}

/// Once anything was written the listing stays at what it showed before, as a lagging GitHub does.
pub struct Lagging<'a> {
    pub inner: Listed<'a>,
    frozen: Mutex<Option<Listing>>,
    wrote: std::sync::atomic::AtomicBool,
}

impl<'a> Lagging<'a> {
    pub fn new(inner: Listed<'a>) -> Self {
        let frozen = Mutex::new(inner.list().ok());
        Self {
            inner,
            frozen,
            wrote: false.into(),
        }
    }
}

impl PackStore for Lagging<'_> {
    fn name(&self) -> &'static str {
        "lagging"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.inner.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        let stale = self.wrote.load(std::sync::atomic::Ordering::SeqCst);
        match (stale, self.frozen.lock().unwrap().clone()) {
            (true, Some(old)) => Ok(old),
            _ => self.inner.list(),
        }
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        self.inner.read_index(id)
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.wrote.store(true, std::sync::atomic::Ordering::SeqCst);
        self.inner.create(tag, index)
    }
    fn append(&self, id: &str, n: u64, e: &str, s: &[Segment], i: &str) -> Result<(), StoreError> {
        self.wrote.store(true, std::sync::atomic::Ordering::SeqCst);
        self.inner.append(id, n, e, s, i)
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.inner.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.inner.delete(id)
    }
}

fn noop() {}

/// One machine: storage dir, scratch dir, the pack store, the old gists and the key.
pub struct Rig {
    pub dir: TempDir,
    pub scratch: TempDir,
    pub mem: MemoryStore,
    pub legacy: LegacyFake,
    pub key: Key,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    pub fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
            scratch: TempDir::new().unwrap(),
            mem: MemoryStore::new(),
            legacy: LegacyFake::new(),
            key: Key::generate(),
        }
    }

    pub fn env<'a>(&'a self, store: &'a dyn PackStore) -> MigrateEnv<'a> {
        MigrateEnv {
            convert_index: &agr::sync::convert_local_index,
            legacy: &self.legacy,
            store,
            key: &self.key,
            writer: "w1",
            target: TARGET,
            hint: None,
            dir: self.dir.path(),
            scratch: self.scratch.path(),
            now: NOW,
            pace: &noop,
        }
    }

    pub fn index_path(&self) -> std::path::PathBuf {
        self.dir.path().join(".agr-sync.json")
    }

    /// A v1 local index, entries are (path, sha256, gen).
    pub fn write_v1_index(&self, entries: &[(&str, &str, u64)]) {
        let one = |(p, sha, gen): &(&str, &str, u64)| {
            let e = format!(
                r#"{{"sha256":"{sha}","remote_id":"gid","synced_at":"t","gen":{gen},"token":"tok","pending":false,"size":0,"mtime_ns":0}}"#
            );
            format!(r#""{p}":{e}"#)
        };
        let all: Vec<String> = entries.iter().map(one).collect();
        let text = format!(r#"{{"version":1,"entries":{{{}}}}}"#, all.join(","));
        std::fs::write(self.index_path(), text).unwrap();
    }

    pub fn index_text(&self) -> String {
        std::fs::read_to_string(self.index_path()).unwrap_or_default()
    }

    /// The session is in a pack already at `gen`, as a migrate that was interrupted left it.
    pub fn put_moved(&self, path: &str, gen: u64, plain: &[u8]) {
        let file = self.scratch.path().join("moved");
        std::fs::write(&file, plain).unwrap();
        let mut repo = Repo::open_for_migrate(&self.mem, &self.key, "w0", TARGET, None).unwrap();
        let item = PushItem {
            path: path.into(),
            gen,
            file,
        };
        repo.push_batch(&[item]).unwrap();
    }

    pub fn merged(&self, path: &str) -> Option<(u64, String)> {
        let repo = Repo::open_for_migrate(&self.mem, &self.key, "w1", TARGET, None).unwrap();
        let m = repo.view().get(path)?;
        Some((m.gen, m.keeper().sha256.clone()))
    }
}

impl LegacyFake {
    /// A v1 session of `key` whose file list GitHub cut off, files may be missing from it.
    pub fn v1_truncated(&self, key: &Key, (path, gen): (&str, u64), plain: &[u8]) -> String {
        let id = self.v1(key, (path, gen), plain, None);
        self.gists
            .lock()
            .unwrap()
            .last_mut()
            .unwrap()
            .info
            .truncated = true;
        id
    }
}
