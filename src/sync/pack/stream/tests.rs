use crate::sync::crypto::blob::BlobError;
use crate::sync::crypto::{sha256_hex, CryptoError, Key};
use crate::sync::pack::index::Entry;
use crate::sync::pack::stream::RangeError;
use crate::sync::pack::stream::{open_range, segment};
use crate::sync::store::{PackStore, Segment};
use crate::sync::testing::MemoryStore;
use std::io::Write;

#[test]
fn segment_len_equals_written_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let key = Key::generate();
    for size in [0usize, 1, 64 * 1024, 64 * 1024 + 1] {
        let file = dir.path().join("s.cast");
        std::fs::write(&file, vec![7u8; size]).unwrap();
        let seg = segment(&key, "a/s.cast", 1, &file).unwrap();
        let mut out = Vec::new();
        seg.write(&mut out).unwrap();
        assert_eq!(seg.len, out.len() as u64, "plain size {size}");
    }
}

/// Appends `plain` as a sealed session to pack `id`, returns its entry.
fn push(store: &MemoryStore, key: &Key, id: &str, plain: &[u8]) -> Entry {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.cast");
    std::fs::write(&file, plain).unwrap();
    let mut sealed = Vec::new();
    segment(key, "a/s.cast", 1, &file)
        .unwrap()
        .write(&mut sealed)
        .unwrap();
    let before = store.list().unwrap().packs[0].len;
    let idx = store.read_index(id).unwrap();
    let write = |w: &mut dyn Write| w.write_all(&sealed);
    let seg = Segment {
        len: sealed.len() as u64,
        write: &write,
    };
    store
        .append(id, before, &idx, &[seg], &format!("{idx}+"))
        .unwrap();
    Entry {
        path: "a/s.cast".into(),
        gen: 1,
        sha256: sha256_hex(plain),
        size: plain.len() as u64,
        offset: before,
        length: sealed.len() as u64,
        deleted: false,
        at: String::new(),
    }
}

fn two_sessions() -> (MemoryStore, Key, String, Entry) {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let id = store.create("tag", "i").unwrap();
    push(&store, &key, &id, b"first session");
    let second = push(&store, &key, &id, b"second session, a bit longer");
    (store, key, id, second)
}

#[test]
fn range_read_opens_the_second_session() {
    let (store, key, id, entry) = two_sessions();
    let mut out = Vec::new();
    open_range(&key, &store, &id, &entry, &mut out).unwrap();
    assert_eq!(
        sha256_hex(&out),
        sha256_hex(b"second session, a bit longer")
    );
}

#[test]
fn blob_under_other_path_or_gen_fails() {
    let (store, key, id, entry) = two_sessions();
    let other_path = Entry {
        path: "b/s.cast".into(),
        ..entry.clone()
    };
    let other_gen = Entry { gen: 2, ..entry };
    for e in [other_path, other_gen] {
        let err = open_range(&key, &store, &id, &e, &mut Vec::new()).err();
        assert!(
            matches!(
                err,
                Some(RangeError::Blob(BlobError::Crypto(
                    CryptoError::Authentication
                )))
            ),
            "{err:?}"
        );
    }
}

#[test]
fn truncated_range_fails() {
    let (store, key, id, entry) = two_sessions();
    let short = Entry {
        length: entry.length - 1,
        ..entry
    };
    let mut out = Vec::new();
    assert!(open_range(&key, &store, &id, &short, &mut out).is_err());
    assert!(out.is_empty());
}
