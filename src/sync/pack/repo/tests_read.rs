use super::tests::{item, packs_of, repo};
use crate::sync::crypto::Key;
use crate::sync::pack::repo::read_pack;
use crate::sync::testing::MemoryStore;

#[test]
fn read_pack_takes_the_key_as_argument() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = repo(&store, &key, "w1");
    repo.push_batch(&[
        item(dir.path(), "a", 1, b"one"),
        item(dir.path(), "b", 1, b"two"),
    ])
    .unwrap();
    let id = packs_of(&store)[0].id.clone();
    let sessions = read_pack(&store, &id, &key).unwrap();
    assert_eq!(sessions.len(), 2);
}
