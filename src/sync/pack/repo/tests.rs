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

use crate::sync::pack::index::{open_index, seal_index, Index};
use crate::sync::store::{PackInfo, StoreError};

pub(super) fn packs_of(store: &dyn PackStore) -> Vec<PackInfo> {
    store.list().unwrap().packs
}

/// The opened index of the pack `info`.
pub(super) fn index_of(store: &dyn PackStore, key: &Key, info: &PackInfo) -> Index {
    open_index(key, &info.tag, &store.read_index(&info.id).unwrap()).unwrap()
}

/// A zero byte append by somebody else, `edit` changes the index.
pub(super) fn tamper(store: &dyn PackStore, key: &Key, info: &PackInfo, edit: impl Fn(&mut Index)) {
    let text = store.read_index(&info.id).unwrap();
    let mut idx = open_index(key, &info.tag, &text).unwrap();
    edit(&mut idx);
    let next = seal_index(key, &idx).unwrap();
    store.append(&info.id, info.len, &text, &[], &next).unwrap();
}

/// Plain size whose sealed segment reaches the target but stays a normal session.
const FILL: usize = TARGET as usize - 10;

#[test]
fn seal_then_next_push_creates_a_pack() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "a", 1, &vec![1; FILL])])
        .unwrap();
    assert_eq!(packs_of(&store).len(), 1);
    repo.push_batch(&[item(dir.path(), "b", 1, b"two")])
        .unwrap();
    assert_eq!(packs_of(&store).len(), 2);
}

#[test]
fn big_session_gets_own_sealed_pack_and_open_pack_stays_open() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "a", 1, b"small")])
        .unwrap();
    let old_id = repo.open_pack().unwrap().to_string();
    repo.push_batch(&[item(dir.path(), "big", 1, &vec![2; TARGET as usize + 1])])
        .unwrap();
    let fresh = Repo::open(&store, &key, "w1", TARGET, None).unwrap();
    let newest = packs_of(&store).pop().unwrap().id;
    let own = fresh.view().pack(&newest).unwrap();
    assert!(own.sealed);
    assert_eq!(repo.open_pack(), Some(old_id.as_str()));
}

#[test]
fn open_pack_found_by_writer_when_local_id_missing() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    repo(&store, &key, "w1")
        .push_batch(&[item(dir.path(), "a", 1, b"x")])
        .unwrap();
    let id = packs_of(&store)[0].id.clone();
    assert_eq!(repo(&store, &key, "w1").open_pack(), Some(id.as_str()));
}

#[test]
fn index_of_other_writer_or_sealed_is_moved() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "a", 1, b"x")]).unwrap();
    tamper(&store, &key, &packs_of(&store)[0], |i| {
        i.writer = "w2".into()
    });
    let err = repo
        .push_batch(&[item(dir.path(), "b", 1, b"y")])
        .unwrap_err();
    assert!(matches!(err, StoreError::Moved(_)), "{err:?}");
}

#[test]
fn own_open_pack_duplicated_starts_a_new_pack() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    repo(&store, &key, "w1")
        .push_batch(&[item(dir.path(), "a", 1, b"x")])
        .unwrap();
    let a = packs_of(&store).remove(0);
    let b = store
        .create(&a.tag, &store.read_index(&a.id).unwrap())
        .unwrap();
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "c", 1, b"z")]).unwrap();
    assert_ne!(repo.open_pack(), Some(a.id.as_str()));
    assert_ne!(repo.open_pack(), Some(b.as_str()));
    assert_eq!(packs_of(&store).len(), 3);
}

#[test]
fn push_of_440_mib_is_six_sealed_and_one_open_pack() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let items: Vec<_> = (0..721)
        .map(|i| item(dir.path(), &format!("s{i}"), 1, &[7u8; 560]))
        .collect();
    repo(&store, &key, "w1").push_batch(&items).unwrap();
    let fresh = repo(&store, &key, "w1");
    let sealed = packs_of(&store)
        .iter()
        .filter(|p| fresh.view().pack(&p.id).unwrap().sealed)
        .count();
    assert_eq!((sealed, packs_of(&store).len() - sealed), (6, 1));
}

#[test]
fn pack_tag_is_the_index_uuid() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    repo(&store, &key, "w1")
        .push_batch(&[item(dir.path(), "a", 1, b"x")])
        .unwrap();
    let info = packs_of(&store).remove(0);
    assert_eq!(info.tag, index_of(&store, &key, &info).pack);
}
