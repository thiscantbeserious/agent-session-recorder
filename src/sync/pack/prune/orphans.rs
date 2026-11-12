//! `--seal-orphans`: idle open packs of other writers, ADR open risk 4.

use super::{Env, Report};
use crate::sync::pack::index::{open_index, seal_index};
use crate::sync::pack::merge::View;
use crate::sync::store::{PackInfo, StoreError};

/// Open packs of other writers idle for longer than this are sealed.
const IDLE_SECS: i64 = 30 * 24 * 60 * 60;
/// A run downloads at most this much for seals, the rest waits for the next run.
pub const BUDGET: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SealPlan {
    pub seal: Vec<String>,
    pub download: u64,
    pub left: usize,
    /// Larger than the whole budget, another run never changes that.
    pub oversized: Vec<String>,
}

/// Our key, not sealed, not ours, not a copy, idle over 30 days, in id order within `budget`.
pub fn seal_plan(view: &View, packs: &[PackInfo], own: &str, now: i64, budget: u64) -> SealPlan {
    let idle = |p: &PackInfo| {
        let at = chrono::DateTime::parse_from_rfc3339(&p.updated_at).map(|t| t.timestamp());
        at.is_ok_and(|t| now - t > IDLE_SECS)
    };
    let orphan = |p: &&PackInfo| {
        let open = view
            .pack(&p.id)
            .is_some_and(|m| !m.sealed && m.writer != own);
        open && !view.is_frozen(&p.id) && idle(p)
    };
    let mut all: Vec<&PackInfo> = packs.iter().filter(orphan).collect();
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let mut plan = SealPlan::default();
    for p in all {
        if p.len > budget {
            plan.oversized.push(p.id.clone());
        } else if plan.download + p.len > budget {
            plan.left += 1;
        } else {
            plan.download += p.len;
            plan.seal.push(p.id.clone());
        }
    }
    plan
}

/// What became of one seal.
pub(super) enum Sealed {
    Done,
    /// The owner appended meanwhile.
    Moved,
    /// The index does not open, wrong key or damaged.
    Unreadable,
}

/// The zero byte append that seals `id`, the one append to a pack of another writer.
/// `Moved` when the owner appended meanwhile, never retried.
pub(super) fn seal_one(env: &Env, id: &str, info: &PackInfo) -> Result<Sealed, StoreError> {
    let text = env.store.read_index(id)?;
    let Ok(mut index) = open_index(env.key, &info.tag, &text) else {
        return Ok(Sealed::Unreadable);
    };
    index.sealed = true;
    let next = seal_index(env.key, &index).map_err(|e| StoreError::Other(e.to_string()))?;
    match env.store.append_transient(id, info.len, &text, &[], &next) {
        Ok(()) => {
            env.store.sealed(id);
            Ok(Sealed::Done)
        }
        Err(StoreError::Moved(_)) => Ok(Sealed::Moved),
        Err(e) => Err(e),
    }
}

/// Seals what the plan names, one line per skipped pack and for what is left.
pub(super) fn seal_all(
    env: &Env,
    plan: &SealPlan,
    packs: &[PackInfo],
    out: &mut Report,
) -> Result<(), StoreError> {
    for id in &plan.seal {
        let info = packs
            .iter()
            .find(|p| &p.id == id)
            .expect("planned from the listing");
        match seal_one(env, id, info)? {
            Sealed::Done => out.sealed += 1,
            Sealed::Moved => out
                .lines
                .push(format!("{id}: changed during the seal, skipped")),
            Sealed::Unreadable => out.lines.push(format!("{id}: index unreadable, skipped")),
        }
    }
    out.lines.extend(oversized_lines(plan));
    if plan.left > 0 {
        out.lines.push(format!("{} left, run again", plan.left));
    }
    Ok(())
}

/// One line per pack that alone exceeds the budget, it needs an explicit action.
pub(super) fn oversized_lines(plan: &SealPlan) -> impl Iterator<Item = String> + '_ {
    let line = |id: &String| format!("{id}: larger than the seal budget, skipped");
    plan.oversized.iter().map(line)
}
