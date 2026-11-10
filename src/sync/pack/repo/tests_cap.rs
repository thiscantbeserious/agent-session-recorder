//! The index cap: no append writes an index text above it, whatever one push or delete asks for.

use super::tests::item;
use super::Repo;
use crate::sync::crypto::Key;
use crate::sync::testing::MemoryStore;

const CAP: usize = 768 * 1024;
const BIG_TARGET: u64 = 512 * 1024 * 1024;

#[test]
fn three_thousand_small_sessions_never_exceed_the_index_cap() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = Repo::open(&store, &key, "w1", BIG_TARGET, None).unwrap();
    let items: Vec<_> = (0..3000)
        .map(|n| {
            item(
                dir.path(),
                &format!("claude-session-recording-of-the-nightly-refactor-run-{n:05}.cast"),
                1,
                b"x",
            )
        })
        .collect();
    repo.push_batch(&items).unwrap();
    let longest = store.index_texts().iter().map(String::len).max().unwrap();
    assert!(longest <= CAP, "{longest}");
}

#[test]
fn tombstone_batch_writes_all_in_one_append() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = Repo::open(&store, &key, "w1", BIG_TARGET, None).unwrap();
    let items: Vec<_> = (0..50)
        .map(|n| item(dir.path(), &format!("s{n}.cast"), 1, b"x"))
        .collect();
    repo.push_batch(&items).unwrap();
    let before = store.appends();
    let paths: Vec<_> = (0..50).map(|n| format!("s{n}.cast")).collect();
    repo.tombstone_batch(&paths).unwrap();
    assert_eq!(store.appends() - before, 1);
}
/// A repo with 3000 live sessions, pushed in batches that keep every index small.
fn pack_of_sessions<'a>(store: &'a MemoryStore, key: &'a Key, dir: &std::path::Path) -> Repo<'a> {
    let mut repo = Repo::open(store, key, "w1", BIG_TARGET, None).unwrap();
    for start in (0..3000).step_by(250) {
        let items: Vec<_> = (start..start + 250)
            .map(|n| {
                item(
                    dir,
                    &format!("claude-session-recording-of-the-nightly-refactor-run-{n:05}.cast"),
                    1,
                    b"x",
                )
            })
            .collect();
        repo.push_batch(&items).unwrap();
    }
    repo
}

#[test]
fn three_thousand_tombstones_split_by_the_index_cap() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = pack_of_sessions(&store, &key, dir.path());
    let before = store.index_texts().len();
    let paths: Vec<_> = (0..3000)
        .map(|n| format!("claude-session-recording-of-the-nightly-refactor-run-{n:05}.cast"))
        .collect();
    repo.tombstone_batch(&paths).unwrap();
    let texts = store.index_texts();
    let longest = texts[before..].iter().map(String::len).max().unwrap();
    assert!(longest <= CAP, "{longest}");
}
