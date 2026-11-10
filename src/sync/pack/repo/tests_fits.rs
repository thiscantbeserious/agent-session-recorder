use super::tests::{index_of, item, packs_of, repo};
use crate::sync::crypto::Key;
use crate::sync::testing::MemoryStore;

#[test]
fn seal_when_fits_is_false() {
    let (store, key, dir) = (
        MemoryStore::with_slots(3),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    let three: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|n| item(dir.path(), n, 1, b"x"))
        .collect();
    repo.push_batch(&three).unwrap();
    let old = packs_of(&store).remove(0);
    repo.push_batch(&[item(dir.path(), "d", 1, b"y")]).unwrap();
    assert!(index_of(&store, &key, &old).sealed);
    assert_eq!(packs_of(&store).len(), 2);
}

#[test]
fn no_file_slot_left_seals_with_zero_byte_append() {
    let (store, key, dir) = (
        MemoryStore::with_slots(1),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "a", 1, b"x")]).unwrap();
    let before = packs_of(&store).remove(0);
    repo.push_batch(&[item(dir.path(), "b", 1, b"y")]).unwrap();
    let old = packs_of(&store)
        .into_iter()
        .find(|p| p.id == before.id)
        .unwrap();
    assert_eq!(old.len, before.len);
    assert!(index_of(&store, &key, &old).sealed);
}

#[test]
fn every_sealing_append_signals_the_store() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "a", 1, b"x")]).unwrap();
    assert!(store.sealed_ids().is_empty());
    let big = vec![1u8; super::tests::TARGET as usize + 1];
    repo.push_batch(&[item(dir.path(), "big", 1, &big)])
        .unwrap();
    assert_eq!(store.sealed_ids(), vec![packs_of(&store)[1].id.clone()]);
}

/// Panics when a push appends more often than `max`, an endless loop shows up as a failure.
fn stop_after(store: &MemoryStore, max: usize) {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    store.before_append(move |_, _| {
        let n = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        assert!(n < max, "the push loops, {n} appends and counting");
    });
}

#[test]
fn chunk_that_never_fits_a_fresh_pack_is_split_not_looped() {
    let (store, key, dir) = (
        MemoryStore::with_slots(1),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    stop_after(&store, 20);
    let mut repo = repo(&store, &key, "w1");
    let two = [
        item(dir.path(), "a", 1, b"x"),
        item(dir.path(), "b", 1, b"y"),
    ];
    repo.push_batch(&two).unwrap();
    let packs_created = packs_of(&store).len();
    assert_eq!(packs_created, 2);
}

#[test]
fn session_that_cannot_fit_a_fresh_pack_is_an_error() {
    let (store, key, dir) = (
        MemoryStore::with_slots(0),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    stop_after(&store, 20);
    let mut repo = repo(&store, &key, "w1");
    let res = repo.push_batch(&[item(dir.path(), "a", 1, b"x")]);
    assert!(res.is_err(), "{res:?}");
    let packs_created = packs_of(&store).len();
    assert_eq!(packs_created, 1);
}

#[test]
fn open_rejects_a_target_outside_the_pack_size_range() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let gib = 1024 * 1024 * 1024;
    let res = super::Repo::open(&store, &key, "w1", gib, None);
    assert!(res.is_err());
}
