use crate::sync::crypto::blob::BlobError;
use crate::sync::crypto::{sha256_hex, CryptoError, Key};
use crate::sync::pack::index::Entry;
use crate::sync::pack::stream::RangeError;
use crate::sync::pack::stream::{open_range, segment};
use crate::sync::store::{PackStore, Segment};
use crate::sync::testing::MemoryStore;
use std::io::{self, Write};

/// Records the size of every `write` call.
#[derive(Default)]
struct Rec {
    data: Vec<u8>,
    writes: Vec<usize>,
}

impl Write for Rec {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.data.extend_from_slice(buf);
        self.writes.push(buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const PACK: &str = "PACKA";
const FRAME: usize = 64 * 1024;

#[test]
fn segment_len_equals_written_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let key = Key::generate();
    for size in [0usize, 1, 64 * 1024, 64 * 1024 + 1] {
        let file = dir.path().join("s.cast");
        std::fs::write(&file, vec![7u8; size]).unwrap();
        let seg = segment(&key, PACK, "a/s.cast", 1, &file).unwrap();
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
    segment(key, PACK, "a/s.cast", 1, &file)
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
    open_range(&key, &store, &id, PACK, &entry, &mut out).unwrap();
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
        let err = open_range(&key, &store, &id, PACK, &e, &mut Vec::new()).err();
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
    assert!(open_range(&key, &store, &id, PACK, &short, &mut out).is_err());
    assert!(out.is_empty());
}

#[test]
fn open_range_streams_frames() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let id = store.create("tag", "i").unwrap();
    let entry = push(&store, &key, &id, &vec![9u8; 3 * FRAME + 5]);
    let mut w = Rec::default();
    open_range(&key, &store, &id, PACK, &entry, &mut w).unwrap();
    assert_eq!(w.writes, vec![FRAME, FRAME, FRAME, 5]);
}

#[test]
fn truncated_multi_frame_range_leaves_only_whole_verified_frames() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let id = store.create("tag", "i").unwrap();
    let entry = push(&store, &key, &id, &vec![9u8; 3 * FRAME + 5]);
    for cut in [1, 10, 20] {
        let short = Entry {
            length: entry.length - cut,
            ..entry.clone()
        };
        let mut w = Rec::default();
        assert!(
            open_range(&key, &store, &id, PACK, &short, &mut w).is_err(),
            "cut {cut}"
        );
        assert_eq!(w.writes, vec![FRAME, FRAME, FRAME], "cut {cut}");
    }
}

#[test]
fn segment_write_returns_the_sealed_digest() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.cast");
    std::fs::write(&file, b"sealed bytes").unwrap();
    let key = Key::generate();
    let seg = segment(&key, PACK, "a/s.cast", 1, &file).unwrap();
    let digest = seg.write(&mut Vec::new()).unwrap();
    assert_eq!(digest.0, sha256_hex(&std::fs::read(&file).unwrap()));
    assert_eq!(digest.1, 12);
}

#[test]
fn same_size_rewrite_is_caught_by_the_digest() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("s.cast");
    std::fs::write(&file, b"before").unwrap();
    let planned = sha256_hex(&std::fs::read(&file).unwrap());
    let key = Key::generate();
    let seg = segment(&key, PACK, "a/s.cast", 1, &file).unwrap();
    std::fs::write(&file, b"after!").unwrap();
    let (digest, _) = seg.write(&mut Vec::new()).unwrap();
    assert_ne!(digest, planned);
}

#[test]
fn segment_copied_to_another_pack_fails() {
    let (store, key, id, entry) = two_sessions();
    let err = open_range(&key, &store, &id, "PACKB", &entry, &mut Vec::new()).unwrap_err();
    assert!(matches!(err, RangeError::Blob(_)), "{err:?}");
}
