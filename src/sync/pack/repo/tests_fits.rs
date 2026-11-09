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
