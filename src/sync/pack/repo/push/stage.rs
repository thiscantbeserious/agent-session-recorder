//! What one append writes and how a session becomes an index entry.

use super::PushItem;
use crate::sync::crypto::blob::sealed_len;
use crate::sync::crypto::sha256_reader;
use crate::sync::pack::index::{Entry, Index};
use crate::sync::pack::stream::{digest_is, FileSegment};
use crate::sync::store::StoreError;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

/// What one append writes: sessions, tombstones as (path, gen) and the seal flag.
pub(in crate::sync::pack::repo) struct Batch<'b> {
    pub(super) staged: &'b [Staged],
    pub(super) tombs: Vec<(String, u64)>,
    pub(super) seal: bool,
}

impl<'b> Batch<'b> {
    pub(super) fn of(staged: &'b [Staged], seal: bool) -> Self {
        Self {
            staged,
            tombs: Vec::new(),
            seal,
        }
    }
}

/// A session with its hash read up front, the sealed write must reproduce it.
#[derive(Clone)]
pub(in crate::sync::pack::repo) struct Staged {
    pub(super) path: String,
    pub(super) gen: u64,
    pub(super) file: PathBuf,
    pub(super) sha256: String,
    pub(super) size: u64,
    pub(super) len: u64,
}

pub(super) fn io_err(e: io::Error) -> StoreError {
    StoreError::Other(e.to_string())
}

pub(super) fn stage(item: &PushItem) -> Result<Staged, StoreError> {
    let (sha256, size) = File::open(&item.file)
        .and_then(sha256_reader)
        .map_err(io_err)?;
    Ok(Staged {
        path: item.path.clone(),
        gen: item.gen,
        file: item.file.clone(),
        sha256,
        size,
        len: sealed_len(size),
    })
}

/// The old index plus one entry per session, ranges laid out behind the old end.
pub(super) fn grown(old: &Index, batch: &Batch) -> Index {
    let mut next = old.clone();
    next.sealed = batch.seal;
    for (path, gen) in &batch.tombs {
        next.entries.push(Entry {
            path: path.clone(),
            gen: *gen,
            sha256: String::new(),
            size: 0,
            offset: next.len,
            length: 0,
            deleted: true,
            at: chrono::Utc::now().to_rfc3339(),
        });
    }
    for s in batch.staged {
        next.entries.push(Entry {
            path: s.path.clone(),
            gen: s.gen,
            sha256: s.sha256.clone(),
            size: s.size,
            offset: next.len,
            length: s.len,
            deleted: false,
            at: chrono::Utc::now().to_rfc3339(),
        });
        next.len += s.len;
    }
    next
}

/// Seals the file and fails when what was sealed is not what the index says.
pub(super) fn writer<'a>(
    seg: &'a FileSegment<'a>,
    s: &'a Staged,
) -> impl Fn(&mut dyn Write) -> io::Result<()> + 'a {
    move |w| {
        if !digest_is(&seg.write(w)?, &s.sha256, s.size) {
            return Err(io::Error::other("session changed while pushing"));
        }
        Ok(())
    }
}

/// How many sessions bring a pack of length `len` to the target, the last one included.
pub(super) fn fill_count(rest: &[Staged], target: u64, len: u64) -> usize {
    let mut sum = len;
    for (i, s) in rest.iter().enumerate() {
        sum += s.len;
        if sum >= target {
            return i + 1;
        }
    }
    rest.len()
}
