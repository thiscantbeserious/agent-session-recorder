use crate::helpers::pack_repo::{v1_desc, Legacy, TARGET};
use agr::sync::testing::MemoryStore;
use agr::sync::testing::{Repo, RepoError};
use agr::sync::Key;

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
