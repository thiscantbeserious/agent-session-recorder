//! Several machines on one pack store, the fresh machine guard, forks and writer ids.

use crate::helpers::pack_engine::{merged, node, opts, three, A, CAST};
use crate::helpers::pack_repo::{v1_desc, Legacy};
use agr::sync::testing::{copy_key, MemoryStore, PackStore, Repo};
use agr::sync::Key;

const NEEDS: &str = "run `agr sync migrate` first";

fn text_of(err: Option<agr::sync::SyncError>) -> String {
    err.map(|e| e.to_string()).unwrap_or_default()
}

#[test]
fn fresh_machine_with_v1_gists_on_remote_requires_migrate() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    let store = Legacy(&mem, vec![v1_desc(&key)]);
    let e = n.engine(&store, &key, "w1");
    let errs = [
        e.push(&opts()).err(),
        e.pull(&opts()).err(),
        e.delete(&[A.to_string()]).err(),
    ];
    for err in errs {
        let text = text_of(err);
        assert!(text.contains(NEEDS), "{text}");
    }
    assert_eq!(mem.appends(), 0);
}

#[test]
fn deleted_local_index_on_machine_a_refuses_the_same_way() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    n.engine(&mem, &key, "w1").push(&opts()).unwrap();
    let _ = std::fs::remove_file(agr::sync::testing::index::path_for(n.dir()));
    let store = Legacy(&mem, vec![v1_desc(&key)]);
    let appends = mem.appends();
    let text = text_of(n.engine(&store, &key, "w1").push(&opts()).err());
    assert!(text.contains(NEEDS), "{text}");
    assert_eq!(mem.appends(), appends);
}

#[test]
fn v1_gist_of_another_key_does_not_block_and_open_for_migrate_skips_the_guard() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    let foreign = Legacy(&mem, vec![v1_desc(&Key::generate())]);
    let push = n.engine(&foreign, &key, "w1").push(&opts());
    assert!(push.is_ok(), "{:?}", push.err());
    let ours = Legacy(&mem, vec![v1_desc(&key)]);
    assert!(Repo::open_for_migrate(&ours, &key, "w1", 1 << 20, None).is_ok());
}

#[test]
fn fork_is_reported_right_after_push() {
    let (a, b, store, key) = (node(), node(), MemoryStore::new(), Key::generate());
    let key_b = copy_key(&key);
    a.session("claude", "x.cast.gz", CAST);
    b.session("claude", "x.cast.gz", &CAST.replace("secret", "other"));
    a.engine(&store, &key, "wa").push(&opts()).unwrap();
    let report = b.engine(&store, &key_b, "wb").push(&opts()).unwrap();
    assert_eq!(report.forks, vec!["claude/x.cast.gz"], "{:?}", report.items);
}

#[test]
fn new_writer_id_after_state_loss() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    n.session("claude", "a.cast.gz", CAST);
    n.engine(&store, &key, "old").push(&opts()).unwrap();
    n.session("claude", "b.cast.gz", &CAST.replace("secret", "b"));
    n.engine(&store, &key, "new").push(&opts()).unwrap();
    assert_eq!(store.list().unwrap().packs.len(), 2);
    let (at, _) = merged(&store, &key, A);
    assert!(at.is_some(), "the old open pack stays merged");
}
