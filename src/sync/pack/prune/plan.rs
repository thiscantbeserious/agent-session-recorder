//! Pure prune plan over a merged view, no store and no IO.

use crate::sync::pack::merge::{Top, View};
use std::collections::{BTreeMap, BTreeSet};

/// Dead share of a pack, in percent, from which it is repacked.
const REPACK_AT_PERCENT: u64 = 20;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// Sealed packs with nothing live in them.
    pub delete: Vec<String>,
    /// Sealed packs whose live sessions are copied, then the pack is deleted.
    pub repack: Vec<String>,
    /// Top gen tombstones the repacked packs carry over.
    pub copy_tombstones: usize,
    /// Live bytes of the repacked packs, what a run downloads and uploads again.
    pub move_bytes: u64,
    /// Dead bytes of every planned pack.
    pub free_bytes: u64,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.delete.is_empty() && self.repack.is_empty()
    }

    pub fn touches(&self, id: &str) -> bool {
        self.delete.iter().chain(&self.repack).any(|p| p == id)
    }
}

/// Live places per pack: the keeper of a path, every version of a fork.
fn live_counts(view: &View) -> BTreeMap<&str, usize> {
    let mut live: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, m) in view.iter() {
        let all = matches!(m.top, Top::Fork);
        let take = if all { m.places.len() } else { 1 };
        for p in &m.places[..take] {
            *live.entry(p.pack.as_str()).or_default() += 1;
        }
    }
    live
}

/// Sealed packs that are not frozen are the only ones planned, the view only holds readable ones.
pub fn plan(view: &View) -> Plan {
    let live = live_counts(view);
    let mut out = Plan::default();
    let mut repacked = BTreeSet::new();
    for (id, &dead) in view.dead_bytes() {
        let Some(meta) = view.pack(id) else { continue };
        if !meta.sealed || view.is_frozen(id) {
            continue;
        }
        if !live.contains_key(id.as_str()) {
            out.delete.push(id.clone());
        } else if dead * 100 >= meta.len * REPACK_AT_PERCENT && dead > 0 {
            out.repack.push(id.clone());
            repacked.insert(id.as_str());
            out.move_bytes += meta.len.saturating_sub(dead);
        } else {
            continue;
        }
        out.free_bytes += dead;
    }
    out.copy_tombstones = (view.iter())
        .filter(|(_, m)| m.is_deleted() && repacked.contains(m.keeper().pack.as_str()))
        .count();
    out
}
