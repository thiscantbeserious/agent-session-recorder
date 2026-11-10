use crate::helpers::pack_repo::{item, repo};
use agr::sync::testing::DirStore;
use agr::sync::testing::PackStore;
use agr::sync::Key;

#[test]
fn dir_store_two_writers_on_one_id_retry_once_and_read_back() {
    let (root, files) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (store, key) = (DirStore::new(root.path()), Key::generate());
    let mut first = repo(&store, &key, "w1");
    first
        .push_batch(&[item(files.path(), "s", 1, b"s")])
        .unwrap();
    let (mut a, mut b) = (repo(&store, &key, "w1"), repo(&store, &key, "w1"));
    a.push_batch(&[item(files.path(), "x", 1, b"x")]).unwrap();
    b.push_batch(&[item(files.path(), "y", 1, b"yy")]).unwrap();
    let out = files.path().join("out.cast");
    repo(&store, &key, "w1").read_session("y", &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"yy");
    assert_eq!(store.list().unwrap().packs.len(), 1);
}

#[test]
fn dir_store_seals_at_target() {
    let (root, files) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (store, key) = (DirStore::new(root.path()), Key::generate());
    let mut r = repo(&store, &key, "w1");
    r.push_batch(&[item(files.path(), "a", 1, &vec![1; 70_000])])
        .unwrap();
    r.push_batch(&[item(files.path(), "b", 1, b"b")]).unwrap();
    assert_eq!(store.list().unwrap().packs.len(), 2);
}
