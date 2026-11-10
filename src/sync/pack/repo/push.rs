//! Writes of one machine: append to the own open pack, seal, retry.

use super::{OpenPack, Repo, RepoError};
use crate::sync::pack::index::{new_pack_uuid, seal_index, Index};
use crate::sync::pack::rules::{place, should_seal, Place};
use crate::sync::pack::stream::segment;
use crate::sync::store::{Segment, StoreError};
use std::io;
use std::path::PathBuf;

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
            self.open = open_of(next);
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
        if self.store.fits(&open.info(), &lens) {
            return Ok(Some(open));
        }
        self.try_append(&open, &Batch::of(&[], true))?;
        self.open = None;
        Ok(None)
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
        let next = self.try_append(&pack, &batch)?;
        self.open = open_of(next);
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
