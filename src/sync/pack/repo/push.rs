//! Writes of one machine: append to the own open pack, seal, retry.

use super::{OpenPack, Repo};
use crate::sync::pack::index::{new_pack_uuid, seal_index, Index};
use crate::sync::pack::rules::{place, should_seal, Place};
use crate::sync::pack::stream::segment;
use crate::sync::store::{Segment, StoreError};
use std::io;
use std::path::PathBuf;

mod cap;
mod copy;
mod retry;
mod stage;

pub(super) use stage::Batch;
use stage::{fill_count, grown, io_err, now, stage, writer, Staged};

/// One session file to push at `gen`.
pub struct PushItem {
    pub path: String,
    pub gen: u64,
    pub file: PathBuf,
}

/// What one pack append carries, the number of sessions and their plain bytes.
#[derive(Debug, Clone, Copy)]
pub struct Append {
    pub sessions: usize,
    pub bytes: u64,
}

impl Append {
    fn of(chunk: &[Staged]) -> Self {
        Self {
            sessions: chunk.len(),
            bytes: chunk.iter().map(|s| s.size).sum(),
        }
    }
}

/// What became of one item of a push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Written to pack `pack`, `sha256` is the hash of the bytes that were appended.
    Pushed { pack: String, sha256: String },
    /// The merged view was ahead of this gen when the append was planned, nothing was written.
    Superseded,
}

/// The open pack after a push: its id and its length, what the engine saves locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRef {
    pub id: String,
    pub len: u64,
}

#[derive(Debug, Default)]
pub struct PushReport {
    pub forks: Vec<String>,
    /// Outcome per item, `(path, gen)` in push order, only items that were reached.
    pub items: Vec<(String, u64, Outcome)>,
    /// The open pack behind the push, `None` when the last append sealed it.
    pub open: Option<OpenRef>,
    /// Pushed, but the listing for the fork check failed, forks are unknown.
    pub check_failed: bool,
}

/// A push that stopped: the error and what was pushed before it.
#[derive(Debug)]
pub struct PushError {
    pub error: StoreError,
    pub pushed: PushReport,
}

impl std::fmt::Display for PushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for PushError {}

impl PushReport {
    /// Items of `chunk` as the append that wrote `next` left them.
    fn record(&mut self, chunk: &[Staged], next: &OpenPack) {
        for s in chunk {
            let hit = next
                .index
                .entries
                .iter()
                .any(|e| e.path == s.path && e.gen == s.gen && e.sha256 == s.sha256);
            let outcome = match hit {
                true => Outcome::Pushed {
                    pack: next.id.clone(),
                    sha256: s.sha256.clone(),
                },
                false => Outcome::Superseded,
            };
            self.items.push((s.path.clone(), s.gen, outcome));
        }
    }
}

impl PushReport {
    /// Tombstones the append that wrote `next` carried. A retry replans each one to a higher gen,
    /// so the landed one is the newest tombstone of the path at or above the planned gen.
    /// `Superseded` when the retry dropped it.
    fn record_tombs(&mut self, tombs: &[(String, u64)], next: &OpenPack) {
        for (path, planned) in tombs {
            let landed = (next.index.entries.iter())
                .filter(|e| e.deleted && &e.path == path && e.gen >= *planned)
                .map(|e| e.gen)
                .max();
            let (gen, outcome) = match landed {
                Some(gen) => (
                    gen,
                    Outcome::Pushed {
                        pack: next.id.clone(),
                        sha256: String::new(),
                    },
                ),
                None => (*planned, Outcome::Superseded),
            };
            self.items.push((path.clone(), gen, outcome));
        }
    }
}

impl Repo<'_> {
    /// Prune copies: like `push_batch`, but no pack is sealed, so the copies stay in open packs
    /// which no prune deletes.
    pub fn push_copies(&mut self, items: &[PushItem]) -> Result<PushReport, PushError> {
        self.keep_open = true;
        let res = self.push_batch(items);
        self.keep_open = false;
        res
    }

    pub fn push_batch(&mut self, items: &[PushItem]) -> Result<PushReport, PushError> {
        self.push_batch_with(items, &mut |_| {})
    }

    /// Like `push_batch`, `announce` runs before every pack append with what that append carries.
    pub fn push_batch_with(
        &mut self,
        items: &[PushItem],
        announce: &mut dyn FnMut(Append),
    ) -> Result<PushReport, PushError> {
        let mut report = PushReport::default();
        match self.push_into(items, &mut report, announce) {
            Ok(()) => Ok(report),
            Err(error) => {
                report.open = self.open_ref();
                Err(PushError {
                    error,
                    pushed: report,
                })
            }
        }
    }

    fn open_ref(&self) -> Option<OpenRef> {
        let o = self.open.as_ref()?;
        Some(OpenRef {
            id: o.id.clone(),
            len: o.index.len,
        })
    }

    fn push_into(
        &mut self,
        items: &[PushItem],
        report: &mut PushReport,
        announce: &mut dyn FnMut(Append),
    ) -> Result<(), StoreError> {
        let at = now();
        let staged = items.iter().map(stage).collect::<Result<Vec<_>, _>>()?;
        let (big, small): (Vec<_>, Vec<_>) = staged
            .into_iter()
            .partition(|s| place(self.target, s.size) == Place::OwnPack);
        for s in &big {
            let pack = self.new_pack()?;
            let one = std::slice::from_ref(s);
            announce(Append::of(one));
            let next = self.try_append(&pack, &Batch::of(one, !self.keep_open, &at))?;
            report.record(one, &next);
        }
        self.push_small(&small, &at, report, announce)?;
        report.open = self.open_ref();
        report.check_failed = self.reload().is_err();
        if !report.check_failed {
            let ours = |p: &&String| items.iter().any(|i| &i.path == *p);
            report.forks = self.view.forks().iter().filter(ours).cloned().collect();
        }
        Ok(())
    }

    /// Fills the open pack up to the target or the index cap, seals it (unless a prune copy, keep_open) and goes on in a new one.
    fn push_small(
        &mut self,
        mut rest: &[Staged],
        at: &str,
        report: &mut PushReport,
        announce: &mut dyn FnMut(Append),
    ) -> Result<(), StoreError> {
        while !rest.is_empty() {
            let len = self.open.as_ref().map_or(0, |o| o.index.len);
            let by_size = fill_count(rest, self.target, len);
            let pack = self.open_or_new()?;
            let by_cap = cap::take_within(self.key, &pack.index, &cap::session_costs(rest, at))?;
            if by_cap == 0 && !pack.index.entries.is_empty() {
                self.seal_open(&pack, at)?;
                continue;
            }
            let n = self.chunk_len(&pack, rest, by_size.min(by_cap.max(1)))?;
            if n == 0 {
                self.seal_open(&pack, at)?;
                continue;
            }
            let (chunk, tail) = rest.split_at(n);
            let sum: u64 = chunk.iter().map(|s| s.len).sum();
            let capped = n == by_cap.max(1) && !tail.is_empty();
            let full = capped || should_seal(self.target, pack.index.len + sum);
            let seal = full && !self.keep_open;
            announce(Append::of(chunk));
            let next = self.try_append(&pack, &Batch::of(chunk, seal, at))?;
            report.record(chunk, &next);
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
    fn seal_open(&mut self, pack: &OpenPack, at: &str) -> Result<(), StoreError> {
        if self.keep_open {
            // a copy pack stays open, a later normal push or seal-orphans seals it
            self.open = None;
            return Ok(());
        }
        self.try_append(pack, &Batch::of(&[], true, at))?;
        self.open = None;
        Ok(())
    }

    /// How many of the first `n` sessions `pack` takes, 0 when a non-empty pack is full and
    /// has to be sealed. The chunk shrinks until it fits, one that does not
    /// fit even alone is an error, a pack is never created for it again.
    fn chunk_len(&self, pack: &OpenPack, rest: &[Staged], n: usize) -> Result<usize, StoreError> {
        let fits = |k: usize| {
            let lens: Vec<u64> = rest[..k].iter().map(|s| s.len).collect();
            self.store.fits(&pack.info(), &lens)
        };
        match (1..=n).rev().find(|&k| fits(k)) {
            Some(k) => Ok(k),
            None if !pack.index.entries.is_empty() => Ok(0),
            None => Err(StoreError::Other(format!(
                "session {} does not fit a fresh pack of this store",
                rest[0].path
            ))),
        }
    }

    /// Appends a tombstone at gen + 1 of the merged entry of `path` to the open pack.
    pub fn tombstone(&mut self, path: &str) -> Result<PushReport, PushError> {
        self.tombstone_batch(&[path.to_string()])
    }

    /// One tombstone per path, each at gen + 1 of the merged entry.
    pub fn tombstone_batch(&mut self, paths: &[String]) -> Result<PushReport, PushError> {
        let mut report = PushReport::default();
        match self.tombs_into(paths, &mut report) {
            Ok(()) => Ok(report),
            Err(error) => {
                report.open = self.open_ref();
                Err(PushError {
                    error,
                    pushed: report,
                })
            }
        }
    }

    fn tombs_into(&mut self, paths: &[String], report: &mut PushReport) -> Result<(), StoreError> {
        let at = now();
        let mut tombs = Vec::new();
        for path in paths {
            let gen = self
                .view
                .get(path)
                .ok_or_else(|| StoreError::Other(format!("no such session: {path}")))?
                .gen;
            tombs.push((path.clone(), gen + 1));
        }
        self.append_tombs(tombs, (&at, false), report)?;
        report.open = self.open_ref();
        report.check_failed = self.reload().is_err();
        Ok(())
    }

    /// The tombstones in as few appends as the index cap allows, the pack that fills up is sealed, unless a prune copy (keep_open).
    fn append_tombs(
        &mut self,
        mut rest: Vec<(String, u64)>,
        (at, copy): (&str, bool),
        report: &mut PushReport,
    ) -> Result<(), StoreError> {
        while !rest.is_empty() {
            let pack = self.open_or_new()?;
            let by_cap = cap::take_within(self.key, &pack.index, &cap::tomb_costs(&rest, at))?;
            if by_cap == 0 && !pack.index.entries.is_empty() {
                self.seal_open(&pack, at)?;
                continue;
            }
            let tail = rest.split_off(by_cap.max(1));
            let seal = !tail.is_empty() && !self.keep_open;
            let sent = rest.clone();
            let batch = Batch {
                staged: &[],
                tombs: rest,
                seal,
                at: at.to_string(),
                copy,
            };
            let next = self.try_append(&pack, &batch)?;
            report.record_tombs(&sent, &next);
            self.open = open_of(next);
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
