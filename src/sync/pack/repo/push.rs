//! Writes of one machine: append to the own open pack, seal, retry.

use super::{OpenPack, Repo, RepoError};
use crate::sync::pack::index::{new_pack_uuid, seal_index, Index};
use crate::sync::pack::rules::{place, should_seal, Place};
use crate::sync::pack::stream::segment;
use crate::sync::store::{Segment, StoreError};
use std::io;
use std::path::PathBuf;

mod cap;
mod retry;
mod stage;

pub(super) use stage::Batch;
use stage::{fill_count, grown, io_err, stage, writer, Staged};

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

    /// Fills the open pack up to the target or the index cap, seals it and goes on in a new one.
    fn push_small(&mut self, mut rest: &[Staged]) -> Result<(), StoreError> {
        while !rest.is_empty() {
            let len = self.open.as_ref().map_or(0, |o| o.index.len);
            let by_size = fill_count(rest, self.target, len);
            let pack = self.open_or_new()?;
            let by_cap = cap::take_within(self.key, &pack.index, &cap::session_costs(rest))?;
            if by_cap == 0 && !pack.index.entries.is_empty() {
                self.seal_open(&pack)?;
                continue;
            }
            let n = by_size.min(by_cap.max(1));
            let (chunk, tail) = rest.split_at(n);
            let Some(pack) = self.pack_with_room(chunk)? else {
                continue;
            };
            let sum: u64 = chunk.iter().map(|s| s.len).sum();
            let capped = n == by_cap.max(1) && !tail.is_empty();
            let seal = capped || should_seal(self.target, pack.index.len + sum);
            let next = self.try_append(&pack, &Batch::of(chunk, seal))?;
            self.open = open_of(next);
            rest = tail;
        }
        Ok(())
    }

    fn open_or_new(&mut self) -> Result<OpenPack, StoreError> {
        if self.open.is_none() {
            self.open = Some(self.new_pack()?);
        }
        Ok(self.open.clone().expect("set above"))
    }

    /// Seals `pack` with a zero byte append, the next chunk starts a new pack.
    fn seal_open(&mut self, pack: &OpenPack) -> Result<(), StoreError> {
        self.try_append(pack, &Batch::of(&[], true))?;
        self.open = None;
        Ok(())
    }

    /// The open pack (or a new one) when `chunk` fits it. A full open pack is sealed with a
    /// zero byte append and `None` comes back, the caller plans the chunk again for a new pack.
    fn pack_with_room(&mut self, chunk: &[Staged]) -> Result<Option<OpenPack>, StoreError> {
        let Some(open) = self.open.clone() else {
            return self.new_pack().map(Some);
        };
        let lens: Vec<u64> = chunk.iter().map(|s| s.len).collect();
        if self.store.fits(&open.info(), &lens) {
            return Ok(Some(open));
        }
        self.seal_open(&open)?;
        Ok(None)
    }

    /// Appends a tombstone at gen + 1 of the merged entry of `path` to the open pack.
    pub fn tombstone(&mut self, path: &str) -> Result<(), RepoError> {
        self.tombstone_batch(&[path.to_string()])
    }

    /// One tombstone per path, each at gen + 1 of the merged entry.
    pub fn tombstone_batch(&mut self, paths: &[String]) -> Result<(), RepoError> {
        let mut tombs = Vec::new();
        for path in paths {
            let gen = self
                .view
                .get(path)
                .ok_or_else(|| RepoError::Unknown(path.clone()))?
                .gen;
            tombs.push((path.clone(), gen + 1));
        }
        self.append_tombs(tombs)?;
        self.reload()?;
        Ok(())
    }

    /// The tombstones in as few appends as the index cap allows, the pack that fills up is sealed.
    fn append_tombs(&mut self, mut rest: Vec<(String, u64)>) -> Result<(), StoreError> {
        while !rest.is_empty() {
            let pack = self.open_or_new()?;
            let by_cap = cap::take_within(self.key, &pack.index, &cap::tomb_costs(&rest))?;
            if by_cap == 0 && !pack.index.entries.is_empty() {
                self.seal_open(&pack)?;
                continue;
            }
            let tail = rest.split_off(by_cap.max(1));
            let seal = !tail.is_empty();
            let batch = Batch {
                staged: &[],
                tombs: rest,
                seal,
            };
            self.open = open_of(self.try_append(&pack, &batch)?);
            rest = tail;
        }
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
            files: 0,
            text,
            index,
        })
    }

    /// One append behind the end of `pack`, `Moved` is the caller's to handle.
    pub(super) fn append_once(
        &self,
        pack: &OpenPack,
        batch: &Batch,
    ) -> Result<OpenPack, StoreError> {
        let staged = batch.staged;
        let lens: Vec<u64> = staged.iter().map(|s| s.len).collect();
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
            files: pack.files + self.store.files_for(&lens),
            text,
            index: next,
        })
    }
}

/// The pack stays open for the next chunk while it is not sealed.
fn open_of(next: OpenPack) -> Option<OpenPack> {
    (!next.index.sealed).then_some(next)
}
