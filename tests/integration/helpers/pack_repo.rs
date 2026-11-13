//! Shared pieces of the pack repo tests.

use agr::sync::testing::MemoryStore;
use agr::sync::testing::{open_index, seal_index, Index, PackInfo, PackStore, PushItem, Repo};
use agr::sync::testing::{seal_meta, Listing, Meta, Segment, StoreError};
use agr::sync::Key;
use std::io::Read;
use std::path::Path;

pub const TARGET: u64 = 64 * 1024;

/// A session file of `plain` bytes in `dir`.
pub fn item(dir: &Path, path: &str, gen: u64, plain: &[u8]) -> PushItem {
    let file = dir.join(format!("{path}.{gen}"));
    std::fs::write(&file, plain).unwrap();
    PushItem {
        path: path.to_string(),
        gen,
        file,
    }
}

pub fn repo<'a>(store: &'a dyn PackStore, key: &'a Key, writer: &str) -> Repo<'a> {
    Repo::open(store, key, writer, TARGET, None).unwrap()
}

pub fn packs_of(store: &dyn PackStore) -> Vec<PackInfo> {
    store.list().unwrap().packs
}

/// The opened index of the pack `info`.
pub fn index_of(store: &dyn PackStore, key: &Key, info: &PackInfo) -> Index {
    open_index(key, &info.tag, &store.read_index(&info.id).unwrap()).unwrap()
}

/// A zero byte append by somebody else, `edit` changes the index.
pub fn tamper(store: &dyn PackStore, key: &Key, info: &PackInfo, edit: impl Fn(&mut Index)) {
    let text = store.read_index(&info.id).unwrap();
    let mut idx = open_index(key, &info.tag, &text).unwrap();
    edit(&mut idx);
    let next = seal_index(key, &idx).unwrap();
    store.append(&info.id, info.len, &text, &[], &next).unwrap();
}

/// Plain size whose sealed segment reaches the target but stays a normal session.
pub const FILL: usize = TARGET as usize - 10;

/// A store whose listing also shows the given `agr:v1` descriptions.
pub struct Legacy<'a>(pub &'a MemoryStore, pub Vec<String>);

impl PackStore for Legacy<'_> {
    fn name(&self) -> &'static str {
        "legacy"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.0.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        let mut l = self.0.list()?;
        l.legacy = self.1.clone();
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

pub fn v1_desc(key: &Key) -> String {
    let meta = Meta {
        v: 1,
        path: "a.cast".into(),
        sha256: "00".repeat(32),
        size: 1,
        gen: 1,
    };
    format!("agr:v1 {}", seal_meta(key, &meta))
}
