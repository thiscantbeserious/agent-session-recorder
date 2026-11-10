//! The sync engine on a `Repo` over the in-memory pack store.

use crate::helpers::pack_engine::{merged, node, opts, overwrite, raw_push, three, CAST, TARGET};
use agr::sync::testing::{MemoryStore, PackStore, Repo};
use agr::sync::Key;

#[test]
fn push_of_three_sessions_is_one_pack() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    let report = n.engine(&store, &key, "w1").push(&opts()).unwrap();
    assert_eq!(store.list().unwrap().packs.len(), 1);
    assert_eq!(report.pushed, 3, "{:?}", report.items);
}

const X: &str = "claude/x.cast.gz";

#[test]
fn overwrite_resolves_fork_with_gen_plus_one() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &key, "w1", (X, 4), "one");
    raw_push(&store, &key, "w2", (X, 4), "two");
    n.session("claude", "x.cast.gz", CAST);
    let plain = n.engine(&store, &key, "w3").push(&opts()).unwrap();
    assert_eq!(
        plain.pushed, 0,
        "a fork is never pushed without --overwrite"
    );
    n.engine(&store, &key, "w3").push(&overwrite()).unwrap();
    let (at, forks) = merged(&store, &key, X);
    assert_eq!(at.map(|(gen, _)| gen), Some(5));
    assert!(forks.is_empty(), "{forks:?}");
}

#[test]
fn overwrite_restores_a_deleted_session() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &key, "w1", (X, 4), "one");
    let mut repo = Repo::open(&store, &key, "w1", TARGET, None).unwrap();
    repo.tombstone(X).unwrap();
    n.session("claude", "x.cast.gz", CAST);
    let plain = n.engine(&store, &key, "w1").push(&opts()).unwrap();
    assert_eq!(plain.pushed, 0, "a tombstone is not pushed back");
    n.engine(&store, &key, "w1").push(&overwrite()).unwrap();
    let (at, _) = merged(&store, &key, X);
    assert_eq!(at, Some((6, false)));
}
