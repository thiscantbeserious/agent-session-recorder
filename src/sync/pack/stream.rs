//! Pack stream: session files sealed as blob v2 segments, and a range read back out of a pack.

use crate::sync::crypto::blob::{
    open_pack_stream, seal_pack_stream, sealed_len, BlobError, Digest,
};
use crate::sync::crypto::Key;
use crate::sync::pack::index::Entry;
use crate::sync::store::{PackStore, StoreError};
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use thiserror::Error;

/// One session file as a sealed segment, `len` is the exact byte count `write` produces.
#[allow(dead_code)] // used by Stage 51 Repo
pub struct FileSegment<'a> {
    pub len: u64,
    key: &'a Key,
    pack: &'a str,
    path: &'a str,
    gen: u64,
    file: &'a Path,
}

/// Seals `file` under `pack`, `path` and `gen` (all in the AAD) once `write` runs.
///
/// `len` comes from the size now, only the digest `write` returns is authoritative: the caller
/// compares it to the planned entry, a same-size rewrite in between changes the digest. A
/// session still being recorded is skipped by caller policy, the engine skips locked sessions
/// (`SkipReason::Locked`, `engine/scan.rs`) before it plans anything.
#[allow(dead_code)] // used by Stage 51 Repo
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

#[allow(dead_code)] // used by Stage 51 Repo
impl FileSegment<'_> {
    /// Streams the sealed blob into `dst`, one frame in memory at a time.
    pub fn write(&self, dst: &mut dyn Write) -> io::Result<Digest> {
        let src = File::open(self.file)?;
        let (hash, size) = seal_pack_stream(self.key, self.pack, self.path, self.gen, src, dst)
            .map_err(io::Error::other)?;
        if sealed_len(size) != self.len {
            return Err(io::Error::other("session file changed while sealing"));
        }
        Ok((hash, size))
    }
}

/// Failures of a range read, the store and the blob layer stay apart.
#[derive(Debug, Error)]
#[allow(dead_code)] // used by Stage 51 Repo
pub enum RangeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Blob(#[from] BlobError),
}

/// Opens the range of `entry` from pack `id` (uuid `pack`) into `dst`, returns plaintext hash and size.
#[allow(dead_code)] // used by Stage 51 Repo
pub fn open_range(
    key: &Key,
    store: &dyn PackStore,
    id: &str,
    pack: &str,
    entry: &Entry,
    dst: &mut dyn Write,
) -> Result<Digest, RangeError> {
    let src = store.read(id, entry.offset, entry.length)?;
    Ok(open_pack_stream(
        key,
        pack,
        &entry.path,
        entry.gen,
        src,
        dst,
    )?)
}

#[cfg(test)]
mod tests;
