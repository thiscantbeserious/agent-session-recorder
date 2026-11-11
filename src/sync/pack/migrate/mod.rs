//! `agr sync migrate`: moves the per-session gists of the old layout into packs.

mod fetch;
mod local;
mod recognise;

pub use crate::sync::store::legacy::{LegacyGist, LegacyRead, NoLegacy};
pub use local::{convert_file, convert_index};
use recognise::is_ours_v1;
pub use recognise::{recognise, Kind};

use crate::sync::crypto::{Key, Meta};
use crate::sync::pack::merge::View;
use crate::sync::pack::repo::{Outcome, PushItem, Repo, RepoError};
use crate::sync::store::{PackStore, StoreError};
use std::path::Path;

pub struct Env<'a> {
    pub legacy: &'a dyn LegacyRead,
    pub store: &'a dyn PackStore,
    pub key: &'a Key,
    pub writer: &'a str,
    pub target: u64,
    pub hint: Option<&'a str>,
    /// Storage directory, only the sync index in it is ever written.
    pub dir: &'a Path,
    /// Private directory for the sessions in flight.
    pub scratch: &'a Path,
    /// Unix seconds from an injected clock.
    pub now: i64,
    /// Called before every delete, spaces them out.
    pub pace: &'a dyn Fn(),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub lines: Vec<String>,
    pub moved: usize,
    pub deleted: usize,
    /// Ids of the gists kept for a mismatch or an unreadable blob.
    pub kept: Vec<String>,
    /// Steps not done because GitHub asked to wait.
    pub left: usize,
    /// A hard error that ended the run, the lines and counts above are what happened before it.
    pub failed: Option<String>,
}

#[derive(Debug)]
pub enum MigrateError {
    List(StoreError),
    Open(RepoError),
    Store(StoreError),
    Local(String),
}

struct Plan {
    ours: Vec<(LegacyGist, Meta)>,
    stubs: Vec<String>,
    kept: usize,
    /// Kept gists of our key, they block every sync until the user deletes them.
    blockers: Vec<String>,
}

fn classify(env: &Env) -> Result<Plan, MigrateError> {
    let gists = env.legacy.list_all().map_err(MigrateError::List)?;
    let mut plan = Plan {
        ours: Vec::new(),
        stubs: Vec::new(),
        kept: 0,
        blockers: Vec::new(),
    };
    for g in gists {
        match recognise(&g, env.key, env.now) {
            Kind::Ours(meta) => plan.ours.push((g, meta)),
            Kind::Stub => plan.stubs.push(g.id),
            Kind::Kept => {
                plan.kept += 1;
                if is_ours_v1(&g, env.key) {
                    plan.blockers.push(g.id);
                }
            }
            Kind::Ignored => {}
        }
    }
    Ok(plan)
}

fn open<'a>(env: &Env<'a>) -> Result<Repo<'a>, MigrateError> {
    let (store, key) = (env.store, env.key);
    Repo::open_for_migrate(store, key, env.writer, env.target, env.hint).map_err(MigrateError::Open)
}

/// Path, sha256 and a gen at least as high are in the merged view.
fn moved(view: &View, m: &Meta) -> bool {
    let at_gen = |e: &crate::sync::pack::merge::Merged| e.gen >= m.gen;
    let has = |e: &crate::sync::pack::merge::Merged| {
        e.places.iter().any(|p| !p.deleted && p.sha256 == m.sha256)
    };
    view.get(&m.path).is_some_and(|e| at_gen(e) && has(e))
}

/// Counts only, nothing is written.
pub fn preview(env: &Env) -> Result<Report, MigrateError> {
    let plan = classify(env)?;
    let repo = open(env)?;
    let (done, todo): (Vec<_>, Vec<_>) = plan.ours.iter().partition(|(_, m)| moved(repo.view(), m));
    let bytes: u64 = todo.iter().map(|(_, m)| m.size).sum();
    let mut line = format!(
        "{} sessions to move ({}), {} already moved, {} empty stubs to delete, {} not ours (kept)",
        todo.len(),
        humansize::format_size(bytes, humansize::BINARY),
        done.len(),
        plan.stubs.len(),
        plan.kept - plan.blockers.len()
    );
    if !plan.blockers.is_empty() {
        line += &format!(
            ", {} blocked (own gist with extra files, fix by hand, sync stays blocked)",
            plan.blockers.len()
        );
    }
    Ok(Report {
        lines: vec![line],
        ..Report::default()
    })
}

/// Bytes of plain sessions that wait in the scratch dir before they are pushed.
const BATCH_BYTES: u64 = 32 * 1024 * 1024;

fn kept_line(id: &str, why: &str) -> String {
    format!("{id} kept, {why}, delete it by hand with `gh gist delete {id}`")
}

/// The view holds the session at this gen or higher with other content, a re-read never fixes it.
fn differs(view: &View, m: &Meta) -> bool {
    view.get(&m.path).is_some_and(|e| e.gen >= m.gen) && !moved(view, m)
}

/// Plain sessions waiting in the scratch dir for the next append.
#[derive(Default)]
struct Waiting {
    items: Vec<PushItem>,
    bytes: u64,
}

impl Waiting {
    fn add(&mut self, meta: &Meta, file: std::path::PathBuf) {
        self.bytes += meta.size;
        self.items.push(PushItem {
            path: meta.path.clone(),
            gen: meta.gen,
            file,
        });
    }
}

/// Reads every session the view does not hold yet and appends it with its own gen.
/// Returns true when GitHub asked to wait.
fn move_all(
    env: &Env,
    plan: &Plan,
    repo: &mut Repo,
    out: &mut Report,
) -> Result<bool, MigrateError> {
    let mut waiting = Waiting::default();
    let todo: Vec<_> = (plan.ours.iter())
        .filter(|(_, m)| !moved(repo.view(), m))
        .collect();
    for (g, meta) in todo {
        match fetch::fetch(env, g, meta) {
            fetch::Fetched::Ready(file) => waiting.add(meta, file),
            fetch::Fetched::Kept(why) => {
                out.lines.push(kept_line(&g.id, &why));
                out.kept.push(g.id.clone());
            }
            fetch::Fetched::Limit => {
                flush(repo, &mut waiting.items, out)?;
                return Ok(true);
            }
        }
        if waiting.bytes >= BATCH_BYTES {
            flush(repo, &mut waiting.items, out)?;
            waiting.bytes = 0;
        }
    }
    flush(repo, &mut waiting.items, out)?;
    Ok(false)
}

fn flush(repo: &mut Repo, batch: &mut Vec<PushItem>, out: &mut Report) -> Result<(), MigrateError> {
    if batch.is_empty() {
        return Ok(());
    }
    let (pushed, failure) = match repo.push_batch(batch) {
        Ok(report) => (report, None),
        Err(e) => (e.pushed, Some(e.error)),
    };
    let landed = pushed
        .items
        .iter()
        .filter(|i| matches!(i.2, Outcome::Pushed { .. }));
    out.moved += landed.count();
    batch.drain(..).for_each(|i| {
        let _ = std::fs::remove_file(i.file);
    });
    failure.map_or(Ok(()), |e| Err(MigrateError::Store(e)))
}

/// Deletes in order, pacing each, a rate limit ends the list and counts what is left. A hard
/// error ends it too and lands in `out.failed`, so the lines of what was deleted are not lost.
fn delete_all(env: &Env, ids: &[String], out: &mut Report) {
    for (i, id) in ids.iter().enumerate() {
        (env.pace)();
        match env.legacy.delete(id) {
            Ok(()) => out.deleted += 1,
            Err(StoreError::NotFound) => out.lines.push(format!("{id} already gone")),
            Err(StoreError::RateLimited) => {
                out.left += ids.len() - i;
                return;
            }
            Err(e) => {
                out.failed = Some(format!("deleting {id} failed: {e}"));
                return;
            }
        }
    }
}

fn lag_line(id: &str) -> String {
    format!("{id} kept, its session is not in the pack index yet, run `agr sync migrate` again")
}

/// The ids that may be deleted: their session is in the fresh view. The others are counted as
/// left (rate limited run) or kept with the reason, a partial view never gets here.
fn verified_ids(after: &Repo, plan: &Plan, limited: bool, out: &mut Report) -> Vec<String> {
    let mut ids = Vec::new();
    let candidates = plan.ours.iter().filter(|(g, _)| !out.kept.contains(&g.id));
    for (g, meta) in candidates.collect::<Vec<_>>() {
        if moved(after.view(), meta) {
            ids.push(g.id.clone());
        } else if limited {
            out.left += 1;
        } else {
            let line = if differs(after.view(), meta) {
                kept_line(&g.id, "the pack index holds other content for its session")
            } else {
                lag_line(&g.id)
            };
            out.lines.push(line);
            out.kept.push(g.id.clone());
        }
    }
    ids
}

/// Moves, verifies and deletes. Every delete waits for a fresh read of the indexes that shows
/// the session, a partial view or a failed re-read ends the run before any delete. A failed
/// append ends it too, but the report with the kept lines is still returned.
pub fn run(env: &Env) -> Result<Report, MigrateError> {
    let plan = classify(env)?;
    let mut repo = open(env)?;
    local::convert_file(env.dir).map_err(MigrateError::Local)?;
    let mut out = Report::default();
    for id in &plan.blockers {
        out.lines
            .push(kept_line(id, "it has files or a path agr did not write"));
        out.kept.push(id.clone());
    }
    let limited = match move_all(env, &plan, &mut repo, &mut out) {
        Ok(limited) => limited,
        // unverified gists count as left below and only verified ones are deleted
        Err(MigrateError::Store(StoreError::RateLimited)) => true,
        Err(MigrateError::Store(e)) => {
            out.failed = Some(format!("appending to the packs failed: {e}"));
            return Ok(out);
        }
        Err(e) => return Err(e),
    };
    let after = open(env)?;
    let mut ids = verified_ids(&after, &plan, limited, &mut out);
    if limited {
        out.left += plan.stubs.len();
    } else {
        ids.extend(plan.stubs.iter().cloned());
    }
    delete_all(env, &ids, &mut out);
    if out.left > 0 {
        out.lines
            .push(format!("{} left, run again later", out.left));
    }
    Ok(out)
}
