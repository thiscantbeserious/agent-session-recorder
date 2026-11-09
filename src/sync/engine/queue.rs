//! Auto push queue `<storage>/.agr-sync-queue/<agent>/<name>.cast.gz`, one empty marker file
//! per session that waits for an auto push.

use std::path::Path;

use anyhow::Result;

pub const QUEUE_DIR: &str = ".agr-sync-queue";

/// Queue `session` (`agent/name.cast.gz`), queuing it twice is one entry.
pub fn enqueue(_storage_dir: &Path, _session: &str) -> Result<()> {
    Ok(())
}

/// Queued sessions, sorted, entries that are not a valid session path are ignored.
pub fn list(_storage_dir: &Path) -> Vec<String> {
    Vec::new()
}

/// Drop the entries of `sessions`, missing ones are fine.
pub fn remove(_storage_dir: &Path, _sessions: &[String]) {}

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
