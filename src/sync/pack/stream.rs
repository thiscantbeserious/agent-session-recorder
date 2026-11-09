//! Pack stream: session files sealed as blob v2 segments, and a range read back out of a pack.

#![allow(dead_code)] // Repo of Stage 51 uses it

use crate::sync::crypto::blob::{open_stream, seal_stream, sealed_len, BlobError, Digest};
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
    pack: &'a str,
    path: &'a str,
    gen: u64,
    file: &'a Path,
}

/// Seals `file` under `path` and `gen` (both in the AAD) once `write` runs.
///
/// `len` comes from the size now, only the digest `write` returns is authoritative: the caller
/// compares it to the planned entry, a same-size rewrite in between changes the digest. A
/// session still being recorded is skipped by caller policy, the engine skips locked sessions
/// (`SkipReason::Locked`, `engine/scan.rs`) before it plans anything.
pub fn segment<'a>(
    key: &'a Key,
    pack: &'a str,
    path: &'a str,
    gen: u64,
    file: &'a Path,
) -> io::Result<FileSegment<'a>> {
    Ok(FileSegment {
        len: sealed_len(std::fs::metadata(file)?.len()),
        key,
        pack,
        path,
        gen,
        file,
    })
}

impl FileSegment<'_> {
    /// Streams the sealed blob into `dst`, one frame in memory at a time.
    pub fn write(&self, dst: &mut dyn Write) -> io::Result<Digest> {
        let src = File::open(self.file)?;
        let (hash, size) =
            seal_stream(self.key, self.path, self.gen, src, dst).map_err(io::Error::other)?;
        if sealed_len(size) != self.len {
            return Err(io::Error::other("session file changed while sealing"));
        }
        Ok((hash, size))
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

/// Opens the range of `entry` from pack `id` into `dst`, returns plaintext hash and size.
pub fn open_range(
    key: &Key,
    store: &dyn PackStore,
    id: &str,
    pack: &str,
    entry: &Entry,
    dst: &mut dyn Write,
) -> Result<Digest, RangeError> {
    let _ = pack; // stub
    let src = store.read(id, entry.offset, entry.length)?;
    Ok(open_stream(key, &entry.path, entry.gen, src, dst)?)
}

#[cfg(test)]
mod tests;
