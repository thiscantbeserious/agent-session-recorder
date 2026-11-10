//! The retry of an append that found its pack moved: plan the batch again, then append once more.

use super::{cap, Batch, Staged};
use crate::sync::pack::index::{open_index, Index};
use crate::sync::pack::merge::{merge, PackIn, View};
use crate::sync::pack::repo::{OpenPack, Repo};
use crate::sync::pack::rules::should_seal;
use crate::sync::store::StoreError;

/// The batch against a newer view: sessions the view is ahead of and tombstones of paths that
/// are gone or dead are dropped, a tombstone goes one above the gen the view now has.
fn replan(view: &View, batch: &Batch) -> (Vec<Staged>, Vec<(String, u64)>) {
    let wanted = |s: &&Staged| view.get(&s.path).is_none_or(|m| m.gen <= s.gen);
    let staged = batch.staged.iter().filter(wanted).cloned().collect();
    let tomb = |(path, _): &(String, u64)| {
        let m = view.get(path).filter(|m| !m.is_deleted())?;
        Some((path.clone(), m.gen + 1))
    };
    (staged, batch.tombs.iter().filter_map(tomb).collect())
}

impl Repo<'_> {
    /// Appends once, on `Moved(current)` plans again against `current` and appends once more.
    /// A sealed pack means a new pack, but only when something is left to write, a pack of
    /// another writer is never appended to.
    pub(super) fn try_append(
        &self,
        pack: &OpenPack,
        batch: &Batch,
    ) -> Result<OpenPack, StoreError> {
        let next = self.append_retrying(pack, batch)?;
        if next.index.sealed {
            self.store.sealed(&next.id);
        }
        Ok(next)
    }

    fn append_retrying(&self, pack: &OpenPack, batch: &Batch) -> Result<OpenPack, StoreError> {
        let current = match self.append_once(pack, batch) {
            Err(StoreError::Moved(current)) => current,
            other => return other,
        };
        let moved = open_index(self.key, &pack.tag, &current)
            .map_err(|e| StoreError::Other(e.to_string()))?;
        if moved.writer != self.writer {
            return Err(StoreError::Moved(current));
        }
        let (staged, tombs) = replan(&self.view_with(&pack.id, &moved), batch);
        let planned = Batch {
            staged: &staged,
            tombs,
            seal: batch.seal,
            at: batch.at.clone(),
        };
        let again = OpenPack {
            text: current,
            files: self.files_now(&pack.id)?.unwrap_or(pack.files),
            index: moved,
            ..pack.clone()
        };
        self.append_replanned(again, planned)
    }

    /// The replanned batch on `again`, a new pack when that one got sealed meanwhile. Nothing
    /// to write appends nothing, only a seal request still seals an open pack.
    fn append_replanned(
        &self,
        again: OpenPack,
        Batch {
            staged,
            tombs,
            seal,
            at,
        }: Batch,
    ) -> Result<OpenPack, StoreError> {
        let idle = staged.is_empty() && tombs.is_empty();
        if idle && (again.index.sealed || !seal) {
            return Ok(again);
        }
        // the seal wish was for the old pack, a new pack decides it again from what it holds
        let lens: Vec<u64> = staged.iter().map(|s| s.len).collect();
        let room = !again.index.sealed && self.store.fits(&again.info(), &lens);
        let fresh = !room || !self.fits_cap(&again, (staged, &tombs, &at))?;
        let (base, seal) = match fresh {
            true => (self.new_pack()?, false),
            false => (again, seal),
        };
        let sum: u64 = staged.iter().map(|s| s.len).sum();
        let full = !staged.is_empty() && should_seal(self.target, base.index.len + sum);
        let batch = Batch {
            staged,
            tombs,
            seal: seal || full,
            at,
        };
        self.append_once(&base, &batch)
    }

    /// Whether the replanned entries still fit under the index cap behind the moved index, the
    /// other tab of this writer may have filled it since the batch was planned.
    fn fits_cap(
        &self,
        pack: &OpenPack,
        (staged, tombs, at): (&[Staged], &[(String, u64)], &str),
    ) -> Result<bool, StoreError> {
        let mut costs = cap::session_costs(staged, at);
        costs.extend(cap::tomb_costs(tombs, at));
        let n = cap::take_within(self.key, &pack.index, &costs)?;
        Ok(n == costs.len())
    }

    /// The view as `self.view` is, with the index of pack `id` replaced by the one that moved.
    fn view_with(&self, id: &str, moved: &Index) -> View {
        let mut ins: Vec<PackIn> = (self.packs().iter())
            .filter(|(other, _)| other.as_str() != id)
            .map(|(other, (_, i))| PackIn {
                id: other.clone(),
                tag: i.pack.clone(),
                index: Ok(i.clone()),
            })
            .collect();
        ins.push(PackIn {
            id: id.to_string(),
            tag: moved.pack.clone(),
            index: Ok(moved.clone()),
        });
        merge(&ins)
    }

    /// Data files of pack `id` as listed now, the other writer added some.
    fn files_now(&self, id: &str) -> Result<Option<usize>, StoreError> {
        let listing = self.store.list()?;
        Ok(listing
            .packs
            .into_iter()
            .find(|p| p.id == id)
            .map(|p| p.files))
    }
}
