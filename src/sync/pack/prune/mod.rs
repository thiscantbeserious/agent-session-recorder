//! Prune: frees the dead bytes of sealed packs.

mod orphans;
mod plan;
mod safe;

use orphans::seal_all;
pub use orphans::{seal_plan, SealPlan, BUDGET};
pub use plan::{plan, Plan};

use crate::sync::crypto::Key;
use crate::sync::pack::merge::{Merged, Top};
use crate::sync::pack::repo::{PushItem, Repo, RepoError};
use crate::sync::store::{PackStore, StoreError};
use std::path::Path;

pub struct Env<'a> {
    pub store: &'a dyn PackStore,
    pub key: &'a Key,
    pub writer: &'a str,
    pub target: u64,
    pub hint: Option<&'a str>,
    /// Private directory for the sessions in flight.
    pub scratch: &'a Path,
    pub dry_run: bool,
    /// Seal idle open packs of other writers first.
    pub seal_orphans: bool,
    /// Unix seconds from an injected clock.
    pub now: i64,
    /// Bytes a run may download for seals.
    pub seal_budget: u64,
    /// Called before every delete, spaces them out.
    pub pace: &'a dyn Fn(),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<String>,
    pub deleted: usize,
    pub copied: usize,
    pub sealed: usize,
    pub move_bytes: u64,
    pub free_bytes: u64,
    pub failed: Option<String>,
}

#[derive(Debug)]
pub enum PruneError {
    Open(RepoError),
    Store(StoreError),
    Local(String),
}

fn open<'a>(env: &Env<'a>) -> Result<Repo<'a>, PruneError> {
    let hint = (env.writer, env.target, env.hint);
    Repo::open_with(env.store, env.key, hint, false).map_err(PruneError::Open)
}

fn size(bytes: u64) -> String {
    humansize::format_size(bytes, humansize::BINARY)
}

/// Rounded to whole MiB, `96 MiB`, the dry run line of the seals.
fn whole_mib(bytes: u64) -> String {
    format!("{} MiB", (bytes + 512 * 1024) / (1024 * 1024))
}

/// Counts only, nothing is written.
fn preview(plan: &Plan) -> Report {
    let line = format!(
        "{} packs to delete, {} to repack, {} to move, {} to free",
        plan.delete.len(),
        plan.repack.len(),
        size(plan.move_bytes),
        size(plan.free_bytes)
    );
    Report {
        lines: vec![line],
        move_bytes: plan.move_bytes,
        free_bytes: plan.free_bytes,
        ..Report::default()
    }
}

/// Bytes of plain sessions that wait in the scratch dir before they are pushed.
const BATCH_BYTES: u64 = 32 * 1024 * 1024;

fn store_err(e: StoreError) -> PruneError {
    PruneError::Store(e)
}

fn flush(repo: &mut Repo, batch: &mut Vec<PushItem>, copied: &mut usize) -> Result<(), PruneError> {
    if batch.is_empty() {
        return Ok(());
    }
    let res = repo.push_copies(batch);
    if res.is_ok() {
        *copied += batch.len();
    }
    batch.drain(..).for_each(|i| {
        let _ = std::fs::remove_file(i.file);
    });
    res.map(drop).map_err(|e| store_err(e.error))
}

/// Decrypts every live session of the repacked packs and seals it again at the same gen, a
/// session is never a byte copy, the new pack has its own tag and AAD.
fn copy_sessions(
    env: &Env,
    repo: &mut Repo,
    ids: &[String],
    copied: &mut usize,
) -> Result<(), PruneError> {
    let mut batch: Vec<PushItem> = Vec::new();
    let (mut bytes, mut n) = (0u64, 0u64);
    let mut todo = Vec::new();
    for (path, m) in repo.view().iter() {
        let take = if matches!(m.top, Top::Fork) {
            m.places.len()
        } else {
            1
        };
        for p in m.places[..take].iter().filter(|p| !p.deleted) {
            if ids.contains(&p.pack) {
                todo.push((path.clone(), m.gen, p.pack.clone(), p.size));
            }
        }
    }
    for (path, gen, pack, size) in todo {
        n += 1;
        let file = env.scratch.join(format!("{n}.copy"));
        repo.read_place_of(&path, &pack, &file)
            .map_err(|e| PruneError::Local(format!("reading {path} from {pack}: {e}")))?;
        batch.push(PushItem { path, gen, file });
        bytes += size;
        if bytes >= BATCH_BYTES {
            flush(repo, &mut batch, copied)?;
            bytes = 0;
        }
    }
    flush(repo, &mut batch, copied)
}

/// Top gen tombstones whose keeper sits in a repacked pack.
fn tombstones(repo: &Repo, ids: &[String]) -> Vec<(String, u64)> {
    let tomb = |(_, m): &(&String, &Merged)| m.is_deleted() && ids.contains(&m.keeper().pack);
    let all = repo.view().iter().filter(tomb);
    all.map(|(path, m)| (path.clone(), m.gen)).collect()
}

fn delete_all(env: &Env, ids: &[String], out: &mut Report) {
    for (i, id) in ids.iter().enumerate() {
        (env.pace)();
        match env.store.delete(id) {
            Ok(()) => out.deleted += 1,
            Err(StoreError::NotFound) => out.lines.push(format!("{id} already gone")),
            Err(StoreError::RateLimited) => {
                let left = ids.len() - i;
                out.lines.push(format!("{left} left, run again later"));
                return;
            }
            Err(e) => {
                out.failed = Some(format!("deleting {id} failed: {e}"));
                return;
            }
        }
    }
}

/// Copies first, then a fresh read of every index, only then the deletes. A partial view or a
/// failed re-read ends the run before any delete.
pub fn run(env: &Env) -> Result<Report, PruneError> {
    let mut repo = open(env)?;
    let mut out = Report::default();
    if env.seal_orphans {
        let packs = env.store.list().map_err(store_err)?.packs;
        let (own, now, budget) = (env.writer, env.now, env.seal_budget);
        let seals = seal_plan(repo.view(), &packs, own, now, budget);
        if env.dry_run {
            let line = format!(
                "{} packs to seal, {} to download",
                seals.seal.len(),
                whole_mib(seals.download)
            );
            out.lines.push(line);
        } else if !seals.seal.is_empty() {
            seal_all(env, &seals, &packs, &mut out).map_err(store_err)?;
            repo = open(env)?;
        }
    }
    let plan = plan(repo.view());
    if env.dry_run || plan.is_empty() {
        out.lines.extend(preview(&plan).lines);
        (out.move_bytes, out.free_bytes) = (plan.move_bytes, plan.free_bytes);
        return Ok(out);
    }
    copy_sessions(env, &mut repo, &plan.repack, &mut out.copied)?;
    let tombs = tombstones(&repo, &plan.repack);
    if !tombs.is_empty() {
        repo.copy_tombstones(&tombs)
            .map_err(|e| store_err(e.error))?;
    }
    let after = open(env)?;
    let planned: Vec<String> = plan.delete.iter().chain(&plan.repack).cloned().collect();
    let ids = safe::deletable(after.view(), &planned);
    if ids.len() < planned.len() {
        let kept = planned.len() - ids.len();
        out.lines.push(format!(
            "{kept} packs kept, a copy is not visible yet, run again"
        ));
    }
    delete_all(env, &ids, &mut out);
    out.move_bytes = plan.move_bytes;
    out.free_bytes = plan.free_bytes;
    Ok(out)
}
