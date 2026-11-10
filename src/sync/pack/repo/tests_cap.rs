//! The index cap: no append writes an index text above it, whatever one push or delete asks for.

use super::tests::item;
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
    let mut repo = super::Repo::open(&store, &key, "w1", BIG_TARGET, None).unwrap();
    let items: Vec<_> = (0..3000)
        .map(|n| {
            item(
                dir.path(),
                &format!("claude-session-recording-{n:05}.cast"),
                1,
                b"x",
            )
        })
        .collect();
    repo.push_batch(&items).unwrap();
    let longest = store.index_texts().iter().map(String::len).max().unwrap();
    assert!(longest <= CAP, "{longest}");
}
