//! `agr sync migrate`: moves the per-session gists of the old layout into packs.

mod fetch;
mod local;
mod recognise;

pub use local::{convert_file, convert_index};
pub use recognise::{recognise, Kind, LegacyGist};

use crate::sync::crypto::{Key, Meta};
use crate::sync::pack::merge::View;
use crate::sync::pack::repo::{Outcome, PushItem, Repo, RepoError};
use crate::sync::store::{PackStore, StoreError};
use std::io::Read;
use std::path::Path;

/// The read side of the old per-session layout, nothing else of it is needed.
pub trait LegacyRead {
    /// Every gist of the user with its files, all pages, no gist cap.
    fn list_all(&self) -> Result<Vec<LegacyGist>, StoreError>;
    /// Manifest token and blob reader of one gist.
    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), StoreError>;
    fn delete(&self, id: &str) -> Result<(), StoreError>;
}

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
}

fn classify(env: &Env) -> Result<Plan, MigrateError> {
    let gists = env.legacy.list_all().map_err(MigrateError::List)?;
    let mut plan = Plan {
        ours: Vec::new(),
        stubs: Vec::new(),
        kept: 0,
    };
    for g in gists {
        match recognise(&g, env.key, env.now) {
            Kind::Ours(meta) => plan.ours.push((g, meta)),
            Kind::Stub => plan.stubs.push(g.id),
            Kind::Kept => plan.kept += 1,
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
    let line = format!(
        "{} sessions to move ({}), {} already moved, {} empty stubs to delete, {} not ours (kept)",
        todo.len(),
        humansize::format_size(bytes, humansize::BINARY),
        done.len(),
        plan.stubs.len(),
        plan.kept
    );
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

/// Reads every session the view does not hold yet and appends it with its own gen.
/// Returns true when GitHub asked to wait.
fn move_all(
    env: &Env,
    plan: &Plan,
    repo: &mut Repo,
    out: &mut Report,
) -> Result<bool, MigrateError> {
    let (mut batch, mut bytes) = (Vec::new(), 0);
    let mut limited = false;
    let todo: Vec<_> = plan
        .ours
        .iter()
        .filter(|(_, m)| !moved(repo.view(), m))
        .collect();
    for (g, meta) in todo {
        match fetch::fetch(env, g, meta) {
            fetch::Fetched::Ready(file) => {
                bytes += meta.size;
                batch.push(PushItem {
                    path: meta.path.clone(),
                    gen: meta.gen,
                    file,
                });
            }
            fetch::Fetched::Kept(why) => {
                out.lines.push(kept_line(&g.id, &why));
                out.kept.push(g.id.clone());
            }
            fetch::Fetched::Limit => {
                limited = true;
                break;
            }
        }
        if bytes >= BATCH_BYTES {
            flush(repo, &mut batch, out)?;
            bytes = 0;
        }
    }
    flush(repo, &mut batch, out)?;
    Ok(limited)
}

fn flush(repo: &mut Repo, batch: &mut Vec<PushItem>, out: &mut Report) -> Result<(), MigrateError> {
    if batch.is_empty() {
        return Ok(());
    }
    let pushed = repo
        .push_batch(batch)
        .map_err(|e| MigrateError::Store(e.error))?;
    let landed = pushed
        .items
        .iter()
        .filter(|i| matches!(i.2, Outcome::Pushed { .. }));
    out.moved += landed.count();
    batch.drain(..).for_each(|i| {
        let _ = std::fs::remove_file(i.file);
    });
    Ok(())
}

/// Deletes in order, pacing each, a rate limit ends the list and counts what is left.
fn delete_all(env: &Env, ids: &[String], out: &mut Report) -> Result<(), MigrateError> {
    for (i, id) in ids.iter().enumerate() {
        (env.pace)();
        match env.legacy.delete(id) {
            Ok(()) | Err(StoreError::NotFound) => out.deleted += 1,
            Err(StoreError::RateLimited) => {
                out.left += ids.len() - i;
                return Ok(());
            }
            Err(e) => return Err(MigrateError::Store(e)),
        }
    }
    Ok(())
}

/// Moves, verifies and deletes. Every delete waits for a fresh read of the indexes that shows
/// the session, a partial view or a failed re-read ends the run before any delete.
pub fn run(env: &Env) -> Result<Report, MigrateError> {
    let plan = classify(env)?;
    let mut repo = open(env)?;
    local::convert_file(env.dir).map_err(MigrateError::Local)?;
    let mut out = Report::default();
    let limited = move_all(env, &plan, &mut repo, &mut out)?;
    let after = open(env)?;
    let mut ids = Vec::new();
    let verified: Vec<_> = plan
        .ours
        .iter()
        .filter(|(g, _)| !out.kept.contains(&g.id))
        .collect();
    for (g, meta) in verified {
        if moved(after.view(), meta) {
            ids.push(g.id.clone());
        } else if limited {
            out.left += 1;
        } else {
            out.lines
                .push(kept_line(&g.id, "its session is not in the pack index"));
            out.kept.push(g.id.clone());
        }
    }
    if !limited {
        ids.extend(plan.stubs.iter().cloned());
    } else {
        out.left += plan.stubs.len();
    }
    delete_all(env, &ids, &mut out)?;
    if out.left > 0 {
        out.lines
            .push(format!("{} left, run again later", out.left));
    }
    Ok(out)
}
