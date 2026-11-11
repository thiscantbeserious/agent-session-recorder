//! Auto push queue `<storage>/.agr-sync-queue/<agent>/<name>.cast.gz`, one empty marker file
//! per session that waits for an auto push.

use std::path::Path;

use anyhow::Result;

use std::collections::BTreeSet;

use super::index;
use super::lock::SyncLock;
use super::observe::{counted, Observer, Step};
use super::plan::{Decision, SkipReason};
use super::run::{Item, Options, Outcome, Report, SyncError};
use crate::sync::pack::rules::valid_path;

pub const QUEUE_DIR: &str = ".agr-sync-queue";

/// Queue `session` (`agent/name.cast.gz`), queuing it twice is one entry. The entry is an
/// empty file, so there is no half written state. `create` without `truncate` makes two racing
/// calls end in the same single file, no temp file is involved.
pub fn enqueue(storage_dir: &Path, session: &str) -> Result<()> {
    anyhow::ensure!(valid_path(session), "not a session path: {session}");
    let path = storage_dir.join(QUEUE_DIR).join(session);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    Ok(())
}

/// Queued sessions, sorted, entries that are not a valid session path are ignored.
pub fn list(storage_dir: &Path) -> Vec<String> {
    let names = |dir: &Path| -> Vec<String> {
        std::fs::read_dir(dir).map_or_else(
            |_| Vec::new(),
            |rd| {
                let files = rd.flatten().map(|e| e.file_name());
                files.filter_map(|n| n.to_str().map(String::from)).collect()
            },
        )
    };
    let root = storage_dir.join(QUEUE_DIR);
    let mut found: Vec<String> = names(&root)
        .into_iter()
        .flat_map(|agent| {
            let files = names(&root.join(&agent));
            files.into_iter().map(move |f| format!("{agent}/{f}"))
        })
        .filter(|s| valid_path(s))
        .collect();
    found.sort();
    found
}

/// Drop the entries of `sessions`, missing ones are fine.
pub fn remove(storage_dir: &Path, sessions: &[String]) {
    for session in sessions.iter().filter(|s| valid_path(s)) {
        let _ = std::fs::remove_file(storage_dir.join(QUEUE_DIR).join(session));
    }
}

/// Done with `item`. A failed one stays for the next run, so does a locked or unreadable one
/// (still recording, maybe a transient error). Everything else leaves the queue, pushed and
/// in sync because they are done, conflicts and the like because only the user can resolve
/// them (the stored error line still tells).
pub(super) fn settle(storage_dir: &Path, item: &Item) {
    let retry = matches!(item.outcome, Outcome::Failed(_))
        && item.decision != Decision::Skip(SkipReason::NotFound)
        || matches!(
            item.decision,
            Decision::Skip(SkipReason::Locked | SkipReason::Unreadable)
        );
    if !retry {
        remove(storage_dir, std::slice::from_ref(&item.path));
    }
}

/// What the auto drain needs from an engine.
pub(super) trait Held {
    fn storage_dir(&self) -> std::path::PathBuf;
    /// One push round for `opts.sessions`, the caller holds the sync lock.
    fn push_held(
        &self,
        opts: &Options,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<Report, SyncError>;
}

/// The `--auto` run, pushes the queue plus the changed synced sessions, never pulls or
/// deletes. `Busy` when another sync holds the lock, the entry stays queued for the holder.
// No lost wakeup: a recording enqueues BEFORE it spawns its child. A child that finds the
// lock held exits quietly, so its entry is already on disk. The holder looks at the queue
// after every round while holding the lock (catches entries added mid-drain) and once more
// AFTER releasing it. A child that saw the lock held did so before the release, so its entry
// is on disk before that last look and gets seen. One that came later takes the lock itself.
// Entries that failed stay queued, `tried` keeps this run from looping on them.
pub(super) fn drain<E: Held + ?Sized>(
    engine: &E,
    session: &str,
    observer: &mut Observer<'_>,
) -> Result<Report, SyncError> {
    let dir = engine.storage_dir();
    let mut observer = counted(0, observer);
    let mut tried = BTreeSet::new();
    let mut total = Report::default();
    let mut next = vec![session.to_string()];
    next.extend(index::maybe_changed(&dir, &index::load(&dir).0));
    loop {
        let lock = match SyncLock::acquire(&dir) {
            Err(SyncError::Busy(_)) if !tried.is_empty() => return Ok(total),
            held => held?,
        };
        drain_held(engine, &mut next, &mut tried, (&mut total, &mut observer))?;
        drop(lock);
        next = list(&dir);
        next.retain(|s| !tried.contains(s));
        if next.is_empty() {
            return Ok(total);
        }
    }
}

/// Push `next`, then whatever was queued meanwhile, until nothing untried is left.
fn drain_held<E: Held + ?Sized>(
    engine: &E,
    next: &mut Vec<String>,
    tried: &mut BTreeSet<String>,
    (total, observer): (&mut Report, &mut dyn FnMut(Step<'_>)),
) -> Result<(), SyncError> {
    let dir = engine.storage_dir();
    loop {
        next.extend(list(&dir));
        next.retain(|s| tried.insert(s.clone()));
        if next.is_empty() {
            return Ok(());
        }
        let sessions = std::mem::take(next);
        let opts = Options {
            sessions,
            ..Options::default()
        };
        total.absorb(engine.push_held(&opts, observer)?);
    }
}
