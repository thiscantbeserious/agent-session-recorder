//! Pack stream: session files sealed as blob v2 segments, and a range read back out of a pack.

#![allow(dead_code)] // Repo of Stage 51 uses it

use crate::sync::crypto::blob::{seal_stream, BlobError};
use crate::sync::crypto::Key;
use crate::sync::pack::index::Entry;
use crate::sync::store::{PackStore, StoreError};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use thiserror::Error;

/// One session file as a sealed segment, `len` is the exact byte count `write` produces.
pub struct FileSegment<'a> {
    pub len: u64,
    key: &'a Key,
    path: &'a str,
    gen: u64,
    file: &'a Path,
}

/// Seals `file` under `path` and `gen` (both in the AAD) once `write` runs.
pub fn segment<'a>(
    key: &'a Key,
    path: &'a str,
    gen: u64,
    file: &'a Path,
) -> io::Result<FileSegment<'a>> {
    Ok(FileSegment {
        len: 0,
        key,
        path,
        gen,
        file,
    })
}

impl FileSegment<'_> {
    /// Streams the sealed blob into `dst`, one frame in memory at a time.
    pub fn write(&self, dst: &mut dyn Write) -> io::Result<()> {
        let src = File::open(self.file)?;
        seal_stream(self.key, self.path, self.gen, src, dst).map_err(io::Error::other)?;
        Ok(())
    }
}

/// Failures of a range read, the store and the blob layer stay apart.
#[derive(Debug, Error)]
pub enum RangeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Blob(#[from] BlobError),
}

/// Opens the range of `entry` from pack `id` into `out`, returns plaintext hash and size.
pub fn open_range(
    _key: &Key,
    _store: &dyn PackStore,
    _id: &str,
    _entry: &Entry,
    _out: &mut Vec<u8>,
) -> Result<(String, u64), RangeError> {
    Ok((String::new(), 0))
}

#[cfg(test)]
mod tests {
    use super::{open_range, segment};
    use crate::sync::crypto::blob::BlobError;
    use crate::sync::crypto::{sha256_hex, CryptoError, Key};
    use crate::sync::pack::index::Entry;
    use crate::sync::pack::stream::RangeError;
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
}
