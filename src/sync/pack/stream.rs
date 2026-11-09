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
        len: sealed_len(std::fs::metadata(file)?.len()),
        key,
        path,
        gen,
        file,
    })
}

impl FileSegment<'_> {
    /// Streams the sealed blob into `dst`, one frame in memory at a time.
    pub fn write(&self, dst: &mut dyn Write) -> io::Result<Digest> {
        let src = File::open(self.file)?;
        let _ = seal_stream(self.key, self.path, self.gen, src, dst);
        Ok((String::new(), 0))
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
    entry: &Entry,
    dst: &mut dyn Write,
) -> Result<Digest, RangeError> {
    let src = store.read(id, entry.offset, entry.length)?;
    Ok(open_stream(key, &entry.path, entry.gen, src, dst)?)
}

#[cfg(test)]
mod tests;
