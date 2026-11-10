//! The sync engine on a `Repo` over the in-memory pack store.

use crate::helpers::pack_engine::{
    index_of, merged, node, opts, overwrite, raw_push, three, write_v1_index, A, CAST, TARGET,
};
use agr::sync::testing::{MemoryStore, PackStore, Repo};
use agr::sync::{Engine, Key};

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

#[test]
fn push_on_v1_index_requires_migrate_first() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    write_v1_index(&n);
    let e = n.engine(&store, &key, "w1");
    let errs = [
        e.push(&opts()).err(),
        e.pull(&opts()).err(),
        e.delete(&[A.to_string()]).err(),
    ];
    assert_eq!(store.appends(), 0);
    for err in errs {
        let text = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(text.contains("run `agr sync migrate` first"), "{text}");
    }
}

#[test]
fn open_pack_saved_after_push_and_cleared_after_seal() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    n.engine(&store, &key, "w1").push(&opts()).unwrap();
    assert_eq!(index_of(&n).open_pack, Some("mem-1".to_string()));
    // a session that brings the open pack to the target seals it
    let target = 64 * 1024;
    let small = Engine::new(&store, &key, &n.storage, "w1", target);
    let noise: Vec<u8> = (0..target as usize).map(|i| (i * 31 % 251) as u8).collect();
    let file = n.dir().join("claude/fill.cast.gz");
    std::fs::write(file, noise).unwrap();
    small.push(&opts()).unwrap();
    assert_eq!(index_of(&n).open_pack, None);
}

#[test]
fn delete_is_a_tombstone() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    n.engine(&store, &key, "w1").push(&opts()).unwrap();
    n.engine(&store, &key, "w1")
        .delete(&[A.to_string()])
        .unwrap();
    let (at, _) = merged(&store, &key, A);
    assert_eq!(at.map(|(_, dead)| dead), Some(true));
    assert!(n.bytes(A).is_some(), "the local file stays");
    assert!(!index_of(&n).entries.contains_key(A));
}

#[test]
fn rollback_below_local_gen_is_refused() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    n.session("claude", "a.cast.gz", CAST);
    let e = n.engine(&store, &key, "w1");
    e.push(&opts()).unwrap();
    n.session("claude", "a.cast.gz", &CAST.replace("secret", "v2"));
    e.push(&opts()).unwrap();
    let before = n.bytes(A);
    // a pack gist deleted outside agr: the view falls back to a lower gen
    let rolled = MemoryStore::new();
    raw_push(&rolled, &key, "w9", (A, 1), "old");
    let report = n.engine(&rolled, &key, "w1").pull(&overwrite()).unwrap();
    assert!(
        report.refused.contains(&A.to_string()),
        "{:?}",
        report.items
    );
    assert_eq!(n.bytes(A), before);
}

#[test]
fn local_absent_remote_deleted_drops_the_index_entry() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    n.engine(&store, &key, "w1").push(&opts()).unwrap();
    n.engine(&store, &key, "w1")
        .delete(&[A.to_string()])
        .unwrap();
    std::fs::remove_file(n.dir().join(A)).unwrap();
    let other = node();
    other.session("claude", "keep.cast.gz", CAST);
    n.engine(&store, &key, "w1").push(&opts()).unwrap();
    assert!(!index_of(&n).entries.contains_key(A));
}

/// Store whose second append fails, the first one lands.
struct FailSecond<'a>(&'a MemoryStore, std::cell::Cell<usize>);

impl PackStore for FailSecond<'_> {
    fn name(&self) -> &'static str {
        "fail-second"
    }
    fn check(&self) -> Result<(), agr::sync::testing::StoreError> {
        self.0.check()
    }
    fn list(&self) -> Result<agr::sync::testing::Listing, agr::sync::testing::StoreError> {
        self.0.list()
    }
    fn read_index(&self, id: &str) -> Result<String, agr::sync::testing::StoreError> {
        self.0.read_index(id)
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, agr::sync::testing::StoreError> {
        self.0.create(tag, index)
    }
    fn append(
        &self,
        id: &str,
        n: u64,
        e: &str,
        s: &[agr::sync::testing::Segment],
        i: &str,
    ) -> Result<(), agr::sync::testing::StoreError> {
        self.1.set(self.1.get() + 1);
        if self.1.get() == 2 {
            return Err(agr::sync::testing::StoreError::RateLimited);
        }
        self.0.append(id, n, e, s, i)
    }
    fn read(
        &self,
        id: &str,
        offset: u64,
        len: u64,
    ) -> Result<Box<dyn std::io::Read>, agr::sync::testing::StoreError> {
        self.0.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), agr::sync::testing::StoreError> {
        self.0.delete(id)
    }
}

#[test]
fn failed_append_mid_batch_keeps_the_index_entries_of_the_earlier_appends() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    let store = FailSecond(&mem, std::cell::Cell::new(0));
    // target 1 byte, every session is bigger and gets its own pack and its own append
    let engine = agr::sync::Engine::new(&store, &key, &n.storage, "w1", 1);
    let err = engine.push(&opts()).err().map(|e| e.to_string());
    assert!(err.is_some(), "a rate limit ends the run");
    assert_eq!(
        index_of(&n).entries.len(),
        1,
        "the landed append is indexed"
    );
}

#[test]
fn pattern_delete_is_one_tombstone_append() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    let all: Vec<String> = (0..11).map(|i| format!("claude/s{i:02}.cast.gz")).collect();
    for p in &all {
        n.session("claude", p.trim_start_matches("claude/"), p);
    }
    n.engine(&store, &key, "w1").push(&opts()).unwrap();
    let before = store.appends();
    let report = n.engine(&store, &key, "w1").delete(&all).unwrap();
    assert_eq!(store.appends() - before, 1, "one append for 11 tombstones");
    assert_eq!(report.deleted, 11);
    for p in &all {
        assert_eq!(merged(&store, &key, p).0, Some((2, true)), "{p}");
        assert!(n.bytes(p).is_some(), "the local file stays");
    }
}
