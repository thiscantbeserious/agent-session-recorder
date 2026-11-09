use super::{PushItem, Repo};
use crate::sync::crypto::Key;
use crate::sync::store::PackStore;
use crate::sync::testing::MemoryStore;
use std::path::Path;

pub(super) const TARGET: u64 = 64 * 1024;

/// A session file of `plain` bytes in `dir`.
pub(super) fn item(dir: &Path, path: &str, gen: u64, plain: &[u8]) -> PushItem {
    let file = dir.join(format!("{path}.{gen}"));
    std::fs::write(&file, plain).unwrap();
    PushItem {
        path: path.to_string(),
        gen,
        file,
    }
}

pub(super) fn repo<'a>(store: &'a dyn PackStore, key: &'a Key, writer: &str) -> Repo<'a> {
    Repo::open(store, key, writer, TARGET, None).unwrap()
}

#[test]
fn push_batch_appends_to_own_open_pack() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "a", 1, b"one")])
        .unwrap();
    repo.push_batch(&[item(dir.path(), "b", 1, b"two")])
        .unwrap();
    assert_eq!(store.list().unwrap().packs.len(), 1);
}
