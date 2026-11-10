//! The index cap: no append writes an index text above it, whatever one push or delete asks for.

use super::tests::item;
use super::Repo;
use crate::sync::crypto::Key;
use crate::sync::store::PackStore;
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

#[test]
fn tombstone_batch_partial_failure_reports_what_landed() {
    let (store, key, dir) = (
        MemoryStore::new(),
        std::sync::Arc::new(Key::generate()),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = pack_of_sessions(&store, &key, dir.path());
    let (calls, other) = (std::sync::atomic::AtomicUsize::new(0), key.clone());
    // a second writer moves the pack under the second append, our own retry refuses it
    store.before_append(move |m, id| {
        if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
            let info = m.list().unwrap().packs.into_iter().find(|p| p.id == id);
            super::tests::tamper(m, &other, &info.unwrap(), |i| i.writer = "w2".into());
        }
    });
    let paths: Vec<_> = (0..3000)
        .map(|n| format!("claude-session-recording-of-the-nightly-refactor-run-{n:05}.cast"))
        .collect();
    let err = repo.tombstone_batch(&paths).unwrap_err();
    let landed = err.pushed.items.len();
    assert!(landed > 0 && landed < 3000, "{landed}");
}

#[test]
fn tombstone_batch_reload_failure_is_check_failed_not_failed() {
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut repo = super::tests::repo(&store, &key, "w1");
    repo.push_batch(&[item(dir.path(), "a", 1, b"x")]).unwrap();
    store.before_append(|m, _| m.fail_next_list());
    let report = repo.tombstone_batch(&["a".to_string()]).unwrap();
    assert!(report.check_failed);
    assert_eq!(report.items.len(), 1);
}
