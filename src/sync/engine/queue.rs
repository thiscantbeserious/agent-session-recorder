//! Auto push queue `<storage>/.agr-sync-queue/<agent>/<name>.cast.gz`, one empty marker file
//! per session that waits for an auto push.

use std::path::Path;

use anyhow::Result;

use std::collections::BTreeSet;

use super::index;
use super::lock::SyncLock;
use super::observe::{counted, Observer, Step};
use super::plan::{Command, Decision, SkipReason};
use super::run::{Engine, Item, Options, Outcome, Report, SyncError};
use super::scan::valid_path;

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

impl Engine<'_> {
    /// The `--auto` run, pushes the queue plus the changed synced sessions, never pulls or
    /// deletes. `Busy` when another sync holds the lock, the entry stays queued for the holder.
    // No lost wakeup: a recording enqueues BEFORE it spawns its child. A child that finds the
    // lock held exits quietly, so its entry is already on disk. The holder looks at the queue
    // after every round while holding the lock (catches entries added mid-drain) and once more
    // AFTER releasing it. A child that saw the lock held did so before the release, so its entry
    // is on disk before that last look and gets seen. One that came later takes the lock itself.
    // Entries that failed stay queued, `tried` keeps this run from looping on them.
    pub fn drain_auto(
        &self,
        session: &str,
        observer: &mut Observer<'_>,
    ) -> Result<Report, SyncError> {
        let dir = self.storage.storage_dir();
        let mut observer = counted(0, observer);
        let mut tried = BTreeSet::new();
        let mut total = Report::default();
        let mut next = vec![session.to_string()];
        next.extend(index::maybe_changed(&dir, &index::load(&dir).0));
        loop {
            let lock = match SyncLock::acquire(&dir) {
                Err(SyncError::Busy) if !tried.is_empty() => return Ok(total),
                held => held?,
            };
            self.drain_held(&mut next, &mut tried, &mut total, &mut observer)?;
            drop(lock);
            next = list(&dir);
            next.retain(|s| !tried.contains(s));
            if next.is_empty() {
                return Ok(total);
            }
        }
    }

    /// Push `next`, then whatever was queued meanwhile, until nothing untried is left.
    fn drain_held(
        &self,
        next: &mut Vec<String>,
        tried: &mut BTreeSet<String>,
        total: &mut Report,
        observer: &mut dyn FnMut(Step<'_>),
    ) -> Result<(), SyncError> {
        let dir = self.storage.storage_dir();
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
            total.absorb(self.sync_held(Command::Push, &opts, observer)?);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: &str = "claude/a.cast.gz";

    fn count_all(dir: &Path) -> usize {
        walk(&dir.join(QUEUE_DIR))
    }

    fn walk(dir: &Path) -> usize {
        std::fs::read_dir(dir).map_or(0, |rd| {
            rd.flatten()
                .map(|e| {
                    if e.path().is_dir() {
                        walk(&e.path())
                    } else {
                        1
                    }
                })
                .sum()
        })
    }

    #[test]
    fn enqueue_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        enqueue(dir.path(), S).unwrap();
        enqueue(dir.path(), S).unwrap();
        assert_eq!(list(dir.path()), [S]);
        assert_eq!(count_all(dir.path()), 1);
    }

    #[test]
    fn concurrent_enqueue_of_one_session_makes_one_complete_entry() {
        let dir = tempfile::tempdir().unwrap();
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| enqueue(dir.path(), S).unwrap());
            }
        });
        assert_eq!(list(dir.path()), [S]);
        assert_eq!(count_all(dir.path()), 1, "a temp or second file was left");
        let file = dir.path().join(QUEUE_DIR).join(S);
        assert_eq!(std::fs::metadata(file).unwrap().len(), 0);
    }

    #[test]
    fn enqueue_refuses_paths_that_could_escape() {
        let dir = tempfile::tempdir().unwrap();
        for bad in [
            "../x.cast.gz",
            "a/b/c.cast.gz",
            ".h/x.cast.gz",
            "x.cast.gz",
            "a/b.cast",
        ] {
            assert!(enqueue(dir.path(), bad).is_err(), "{bad}");
        }
        assert_eq!(count_all(dir.path()), 0);
    }

    #[test]
    fn list_ignores_junk_and_remove_drops_entries() {
        let dir = tempfile::tempdir().unwrap();
        enqueue(dir.path(), S).unwrap();
        enqueue(dir.path(), "codex/b.cast.gz").unwrap();
        let root = dir.path().join(QUEUE_DIR);
        std::fs::create_dir_all(root.join("claude")).unwrap();
        std::fs::write(root.join("stray.cast.gz"), "").unwrap();
        std::fs::write(root.join("claude/.hidden.cast.gz"), "").unwrap();
        std::fs::write(root.join("claude/notes.txt"), "").unwrap();
        assert_eq!(list(dir.path()), [S, "codex/b.cast.gz"]);
        remove(dir.path(), &[S.to_string(), "../../etc/x.cast.gz".into()]);
        assert_eq!(list(dir.path()), ["codex/b.cast.gz"]);
    }
}
