//! Writes of one machine: append to the own open pack, seal, retry.

use super::{OpenPack, Repo};
use crate::sync::crypto::blob::sealed_len;
use crate::sync::crypto::sha256_reader;
use crate::sync::pack::index::{new_pack_uuid, seal_index, Entry, Index};
use crate::sync::pack::stream::{segment, FileSegment};
use crate::sync::store::{Segment, StoreError};
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

/// One session file to push at `gen`.
pub struct PushItem {
    pub path: String,
    pub gen: u64,
    pub file: PathBuf,
}

#[derive(Debug, Default)]
pub struct PushReport {
    pub forks: Vec<String>,
}

/// A session with its hash read up front, the sealed write must reproduce it.
struct Staged {
    path: String,
    gen: u64,
    file: PathBuf,
    sha256: String,
    size: u64,
    len: u64,
}

fn io_err(e: io::Error) -> StoreError {
    StoreError::Other(e.to_string())
}

fn stage(item: &PushItem) -> Result<Staged, StoreError> {
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

impl Repo<'_> {
    pub fn push_batch(&mut self, items: &[PushItem]) -> Result<PushReport, StoreError> {
        let staged = items.iter().map(stage).collect::<Result<Vec<_>, _>>()?;
        let pack = self.new_pack()?;
        self.try_append(&pack, &staged)?;
        Ok(PushReport::default())
    }

    fn new_pack(&self) -> Result<OpenPack, StoreError> {
        let tag = new_pack_uuid();
        let index = Index {
            v: 3,
            pack: tag.clone(),
            writer: self.writer.clone(),
            sealed: false,
            len: 0,
            entries: Vec::new(),
        };
        let text = seal_index(self.key, &index).map_err(|e| StoreError::Other(e.to_string()))?;
        let id = self.store.create(&tag, &text)?;
        Ok(OpenPack {
            id,
            tag,
            text,
            index,
        })
    }

    /// One append of `staged` behind the end of `pack`, `Moved` is the caller's to handle.
    fn try_append(&self, pack: &OpenPack, staged: &[Staged]) -> Result<OpenPack, StoreError> {
        let segs = staged
            .iter()
            .map(|s| segment(self.key, &pack.tag, &s.path, s.gen, &s.file))
            .collect::<io::Result<Vec<_>>>()
            .map_err(io_err)?;
        let next = grown(&pack.index, staged);
        let text = seal_index(self.key, &next).map_err(|e| StoreError::Other(e.to_string()))?;
        let writes: Vec<_> = segs.iter().zip(staged).map(|(g, s)| writer(g, s)).collect();
        let segments: Vec<Segment> = writes
            .iter()
            .zip(&segs)
            .map(|(write, g)| Segment { len: g.len, write })
            .collect();
        self.store
            .append(&pack.id, pack.index.len, &pack.text, &segments, &text)?;
        Ok(OpenPack {
            id: pack.id.clone(),
            tag: pack.tag.clone(),
            text,
            index: next,
        })
    }
}

/// The old index plus one entry per session, ranges laid out behind the old end.
fn grown(old: &Index, staged: &[Staged]) -> Index {
    let mut next = old.clone();
    for s in staged {
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
fn writer<'a>(
    seg: &'a FileSegment<'a>,
    s: &'a Staged,
) -> impl Fn(&mut dyn Write) -> io::Result<()> + 'a {
    move |w| {
        let (hash, size) = seg.write(w)?;
        if hash != s.sha256 || size != s.size {
            return Err(io::Error::other("session changed while pushing"));
        }
        Ok(())
    }
}
