//! Writes of one machine: append to the own open pack, seal, retry.

use super::{OpenPack, Repo, RepoError};
use crate::sync::crypto::blob::sealed_len;
use crate::sync::crypto::sha256_reader;
use crate::sync::pack::index::{new_pack_uuid, open_index, seal_index, Entry, Index};
use crate::sync::pack::rules::{place, should_seal, Place};
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

/// What one append writes: sessions, tombstones as (path, gen) and the seal flag.
pub(super) struct Batch<'b> {
    staged: &'b [Staged],
    tombs: Vec<(String, u64)>,
    seal: bool,
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
pub(super) struct Staged {
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
        let (big, small): (Vec<_>, Vec<_>) = staged
            .into_iter()
            .partition(|s| place(self.target, s.size) == Place::OwnPack);
        for s in &big {
            let pack = self.new_pack()?;
            self.try_append(&pack, &Batch::of(std::slice::from_ref(s), true))?;
        }
        self.push_small(&small)?;
        self.reload()?;
        let ours = |p: &String| items.iter().any(|i| &i.path == p);
        let forks = self
            .view
            .forks()
            .iter()
            .filter(|p| ours(p))
            .cloned()
            .collect();
        Ok(PushReport { forks })
    }

    /// Fills the open pack up to the target, seals it and goes on in a new one.
    fn push_small(&mut self, mut rest: &[Staged]) -> Result<(), StoreError> {
        while !rest.is_empty() {
            let len = self.open.as_ref().map_or(0, |o| o.index.len);
            let (chunk, tail) = rest.split_at(fill_count(rest, self.target, len));
            let Some(pack) = self.pack_with_room(chunk)? else {
                continue;
            };
            let sum: u64 = chunk.iter().map(|s| s.len).sum();
            let seal = should_seal(self.target, pack.index.len + sum);
            let next = self.try_append(&pack, &Batch::of(chunk, seal))?;
            self.open = (!seal).then_some(next);
            rest = tail;
        }
        Ok(())
    }

    /// The open pack (or a new one) when `chunk` fits it. A full open pack is sealed with a
    /// zero byte append and `None` comes back, the caller plans the chunk again for a new pack.
    fn pack_with_room(&mut self, chunk: &[Staged]) -> Result<Option<OpenPack>, StoreError> {
        let Some(open) = self.open.clone() else {
            return self.new_pack().map(Some);
        };
        let lens: Vec<u64> = chunk.iter().map(|s| s.len).collect();
        if self.has_room(&open, &lens)? {
            return Ok(Some(open));
        }
        self.try_append(&open, &Batch::of(&[], true))?;
        self.open = None;
        Ok(None)
    }

    fn has_room(&self, open: &OpenPack, lens: &[u64]) -> Result<bool, StoreError> {
        let packs = self.store.list()?.packs;
        Ok(packs
            .iter()
            .find(|p| p.id == open.id)
            .is_none_or(|p| self.store.fits(p, lens)))
    }

    /// Appends a tombstone at gen + 1 of the merged entry of `path` to the open pack.
    pub fn tombstone(&mut self, path: &str) -> Result<(), RepoError> {
        let gen = self
            .view
            .get(path)
            .ok_or_else(|| RepoError::Unknown(path.into()))?
            .gen;
        let pack = match self.open.clone() {
            Some(p) => p,
            None => self.new_pack()?,
        };
        let batch = Batch {
            staged: &[],
            tombs: vec![(path.to_string(), gen + 1)],
            seal: false,
        };
        self.open = Some(self.try_append(&pack, &batch)?);
        self.reload()?;
        Ok(())
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

    /// Appends once, on `Moved(current)` plans again against `current` and appends once more.
    /// A sealed pack means a new pack, a pack of another writer is never appended to.
    fn try_append(&self, pack: &OpenPack, batch: &Batch) -> Result<OpenPack, StoreError> {
        let StoreError::Moved(current) = (match self.append_once(pack, batch) {
            Err(e @ StoreError::Moved(_)) => e,
            other => return other,
        }) else {
            unreachable!("matched Moved above")
        };
        let moved = open_index(self.key, &pack.tag, &current)
            .map_err(|e| StoreError::Other(e.to_string()))?;
        if moved.writer != self.writer {
            return Err(StoreError::Moved(current));
        }
        if moved.sealed {
            return self.append_once(&self.new_pack()?, batch);
        }
        let again = OpenPack {
            text: current,
            index: moved,
            ..pack.clone()
        };
        self.append_once(&again, batch)
    }

    /// One append behind the end of `pack`, `Moved` is the caller's to handle.
    pub(super) fn append_once(
        &self,
        pack: &OpenPack,
        batch: &Batch,
    ) -> Result<OpenPack, StoreError> {
        let staged = batch.staged;
        let segs = staged
            .iter()
            .map(|s| segment(self.key, &pack.tag, &s.path, s.gen, &s.file))
            .collect::<io::Result<Vec<_>>>()
            .map_err(io_err)?;
        let next = grown(&pack.index, batch);
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
fn grown(old: &Index, batch: &Batch) -> Index {
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

/// How many sessions bring a pack of length `len` to the target, the last one included.
fn fill_count(rest: &[Staged], target: u64, len: u64) -> usize {
    let mut sum = len;
    for (i, s) in rest.iter().enumerate() {
        sum += s.len;
        if sum >= target {
            return i + 1;
        }
    }
    rest.len()
}
