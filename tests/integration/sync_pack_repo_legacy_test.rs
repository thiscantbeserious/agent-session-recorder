use crate::helpers::pack_repo::TARGET;
use agr::sync::testing::MemoryStore;
use agr::sync::testing::{seal_meta, Meta};
use agr::sync::testing::{Listing, PackStore, Segment, StoreError};
use agr::sync::testing::{Repo, RepoError};
use agr::sync::Key;
use std::io::Read;

/// A store whose listing also shows the given `agr:v1` descriptions.
struct Legacy<'a>(&'a MemoryStore, Vec<String>);

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

fn v1_desc(key: &Key) -> String {
    let meta = Meta {
        v: 1,
        path: "a.cast".into(),
        sha256: "00".repeat(32),
        size: 1,
        gen: 1,
    };
    format!("agr:v1 {}", seal_meta(key, &meta))
}

#[test]
fn open_refuses_with_a_v1_gist_of_our_key_and_migrate_skips_the_guard() {
    let (mem, key) = (MemoryStore::new(), Key::generate());
    let store = Legacy(&mem, vec![v1_desc(&key)]);
    let err = Repo::open(&store, &key, "w1", TARGET, None).err();
    assert!(matches!(err, Some(RepoError::NeedsMigrate)), "{err:?}");
    assert!(Repo::open_for_migrate(&store, &key, "w1", TARGET, None).is_ok());
}

#[test]
fn v1_gist_of_another_key_does_not_block() {
    let (mem, key) = (MemoryStore::new(), Key::generate());
    let store = Legacy(&mem, vec![v1_desc(&Key::generate())]);
    assert!(Repo::open(&store, &key, "w1", TARGET, None).is_ok());
}
