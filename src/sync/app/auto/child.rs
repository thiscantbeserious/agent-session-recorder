//! The `--auto` child run and the error line it leaves in the state file.

use crate::config::state::State;
use crate::sync::engine::index;
use crate::sync::engine::run::{Engine, Options, Outcome, Report, SyncError};

/// One line for the state file after an `--auto` run for `session`, `None` when it
/// went fine. A busy lock means the session was not pushed, so that is reported too.
pub fn error_line(result: &Result<Report, SyncError>, session: &str) -> Option<String> {
    let report = match result {
        Err(SyncError::Busy) => {
            return Some(format!(
                "auto push of {session} skipped, another sync was running, run agr sync push"
            ))
        }
        Err(e) => return Some(format!("auto push failed: {e}")),
        Ok(report) => report,
    };
    let item = report.items.iter().find(|i| {
        matches!(i.outcome, Outcome::Failed(_)) || matches!(i.outcome, Outcome::Skipped(_))
    })?;
    let (Outcome::Failed(why) | Outcome::Skipped(why)) = &item.outcome else {
        return None;
    };
    Some(format!("auto push of {} failed: {why}", item.path))
}

/// The stored error as a warning line, clearing it from `state` (caller saves).
pub fn take_error_line(state: &mut State) -> Option<String> {
    let error = state.last_auto_push_error.take()?;
    Some(format!(
        "Last automatic push failed: {error} (see `agr sync status`)"
    ))
}

/// The `--auto` run, pushes the new session and the synced ones that changed since (markers,
/// optimize), never pulls or deletes, a conflict is skipped (no overwrite) and reported.
// ponytail: only the same path with other content counts as changed. A renamed file is a new
// path with no index entry and its old path is gone, so a rename is left to a manual
// `agr sync push` (pushing it here would upload a second copy). Selection is size and mtime from
// the index, so only those files get hashed, a touched but identical file is hashed each time.
pub fn run(engine: &Engine<'_>, session: &str) -> Result<Report, SyncError> {
    let (index, _) = index::load(&engine.storage.storage_dir());
    let mut sessions = vec![session.to_string()];
    for path in index::maybe_changed(&engine.storage.storage_dir(), &index) {
        if path != session {
            sessions.push(path);
        }
    }
    engine.push(&Options {
        sessions,
        ..Options::default()
    })
}

#[cfg(test)]
mod tests {
    use super::{error_line, run, take_error_line, Engine, Outcome, Report, State, SyncError};
    use crate::config::Config;
    use crate::sync::app::auto::tests::session;
    use crate::sync::engine::plan::Decision;
    use crate::sync::engine::run::Item;
    use std::fs;

    fn item(outcome: Outcome, decision: Decision) -> Item {
        Item {
            path: "claude/s.cast.gz".into(),
            decision,
            outcome,
            size: 0,
            on_remote: false,
            on_local: true,
            local_hash: None,
            remote_hash: None,
            remote_size: 0,
            remote_id: None,
            pending: false,
            duplicates: Vec::new(),
        }
    }

    fn report(item: Item, failed: usize, skipped: usize, conflicts: usize) -> Report {
        Report {
            items: vec![item],
            failed,
            skipped,
            conflicts,
            ..Report::default()
        }
    }

    #[test]
    fn auto_run_pushes_only_the_session_never_pulls_and_skips_conflicts() {
        use crate::sync::backend::memory::MemoryBackend;
        use crate::sync::backend::SyncBackend;
        use crate::sync::crypto::blob::seal_blob;
        use crate::sync::crypto::{seal_meta, sha256_hex, Key, Meta};
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.storage.directory = dir.path().to_string_lossy().into_owned();
        let storage = crate::StorageManager::new(cfg);
        let key = Key::generate();
        let backend = MemoryBackend::new();
        let remote_only = Meta {
            v: 2,
            path: "claude/other.cast.gz".into(),
            sha256: sha256_hex(b"other"),
            size: 5,
            gen: 1,
        };
        backend.put(
            None,
            &seal_meta(&key, &remote_only),
            &seal_blob(&key, &remote_only.path, 1, b"other"),
        );
        let path = session(dir.path());
        let engine = Engine {
            backend: &backend,
            key: &key,
            storage: &storage,
        };
        let report = run(&engine, "claude/s.cast.gz").unwrap();
        assert_eq!((report.pushed, report.pulled, report.deleted), (1, 0, 0));
        assert!(!dir.path().join("claude/other.cast.gz").exists());
        assert_eq!(backend.len(), 2);
        // changed on both sides is a conflict, skipped, nothing overwritten
        fs::write(&path, b"changed locally").unwrap();
        let moved = Meta {
            v: 2,
            path: "claude/s.cast.gz".into(),
            sha256: sha256_hex(b"changed remotely"),
            size: 16,
            gen: 1,
        };
        let id = backend
            .list()
            .unwrap()
            .into_iter()
            .find(|e| e.id != "mem-1")
            .unwrap()
            .id;
        backend.put(
            Some(&id),
            &seal_meta(&key, &moved),
            &seal_blob(&key, &moved.path, 1, b"changed remotely"),
        );
        let report = run(&engine, "claude/s.cast.gz").unwrap();
        assert_eq!((report.pushed, report.conflicts), (0, 1));
        assert!(error_line(&Ok(report), "s").unwrap().contains("conflict"));
        assert_eq!(fs::read(&path).unwrap(), b"changed locally");
    }

    /// Storage with `claude/<name>.cast.gz` files, all pushed once by a manual push.
    struct World {
        dir: tempfile::TempDir,
        backend: crate::sync::backend::memory::MemoryBackend,
        key: crate::sync::crypto::Key,
        storage: crate::StorageManager,
    }

    impl World {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let mut cfg = Config::default();
            cfg.storage.directory = dir.path().to_string_lossy().into_owned();
            Self {
                storage: crate::StorageManager::new(cfg),
                backend: crate::sync::backend::memory::MemoryBackend::new(),
                key: crate::sync::crypto::Key::generate(),
                dir,
            }
        }

        fn engine(&self) -> Engine<'_> {
            Engine {
                backend: &self.backend,
                key: &self.key,
                storage: &self.storage,
            }
        }

        fn write(&self, name: &str, body: &[u8]) {
            fs::create_dir_all(self.dir.path().join("claude")).unwrap();
            fs::write(self.dir.path().join(format!("claude/{name}.cast.gz")), body).unwrap();
        }

        fn manual_push(&self, name: &str) {
            let opts = crate::sync::engine::run::Options {
                sessions: vec![format!("claude/{name}.cast.gz")],
                ..Default::default()
            };
            assert_eq!(self.engine().push(&opts).unwrap().pushed, 1);
        }

        fn auto(&self, name: &str) -> Report {
            run(&self.engine(), &format!("claude/{name}.cast.gz")).unwrap()
        }
    }

    #[test]
    fn auto_run_also_pushes_a_synced_session_changed_since() {
        let w = World::new();
        w.write("old", b"v1");
        w.manual_push("old");
        w.write("old", b"v2 with markers");
        w.write("new", b"fresh");
        let report = w.auto("new");
        assert_eq!(report.pushed, 2, "{:?}", report.items);
    }

    #[test]
    fn auto_run_leaves_never_synced_older_sessions_alone() {
        let w = World::new();
        w.write("never", b"old");
        w.write("new", b"fresh");
        assert_eq!(w.auto("new").pushed, 1);
        assert_eq!(w.backend.len(), 1);
    }

    #[test]
    fn auto_run_does_not_repush_an_unchanged_synced_session() {
        let w = World::new();
        w.write("old", b"v1");
        w.manual_push("old");
        w.write("new", b"fresh");
        assert_eq!(w.auto("new").pushed, 1);
        assert_eq!(w.auto("new").pushed, 0);
    }

    #[test]
    fn an_identical_rewrite_is_hashed_once_then_restamped() {
        let w = World::new();
        w.write("old", b"v1");
        w.manual_push("old");
        w.write("old", b"v1"); // same bytes, new mtime
        w.write("new", b"fresh");
        assert_eq!(w.auto("new").pushed, 1);
        let index = crate::sync::engine::index::load(w.dir.path()).0;
        let changed = crate::sync::engine::index::maybe_changed(w.dir.path(), &index);
        assert!(changed.is_empty(), "still re-hashed every run: {changed:?}");
    }

    #[test]
    fn a_renamed_synced_session_is_not_pushed_twice() {
        let w = World::new();
        w.write("old", b"v1");
        w.manual_push("old");
        let dir = w.dir.path().join("claude");
        fs::rename(dir.join("old.cast.gz"), dir.join("renamed.cast.gz")).unwrap();
        w.write("new", b"fresh");
        assert_eq!(w.auto("new").pushed, 1);
        assert!(!w
            .auto("new")
            .items
            .iter()
            .any(|i| i.path.contains("renamed")));
    }

    #[test]
    fn take_error_line_returns_and_clears_the_stored_error() {
        let mut state = State {
            last_auto_push_error: Some("offline".into()),
            ..State::default()
        };
        let line = take_error_line(&mut state).unwrap();
        assert!(
            line.contains("offline") && line.contains("agr sync status"),
            "{line}"
        );
        assert_eq!(state.last_auto_push_error, None);
        assert_eq!(take_error_line(&mut state), None);
    }

    #[test]
    fn error_line_keeps_the_rate_limit_text_with_sessions_left() {
        let line = error_line(&Err(SyncError::RateLimited { left: 4 }), "s").unwrap();
        assert_eq!(
            line,
            "auto push failed: GitHub rate limit reached, sync stopped, 4 left, please run again later"
        );
    }

    #[test]
    fn error_line_is_none_for_success() {
        let ok = report(item(Outcome::Done(1), Decision::Push), 0, 0, 0);
        assert_eq!(error_line(&Ok(ok), "s"), None);
    }

    #[test]
    fn error_line_names_the_skipped_session_when_busy() {
        assert!(error_line(&Err(SyncError::Busy), "s").is_some());
    }

    #[test]
    fn error_line_names_failures_conflicts_and_skips_on_one_line() {
        let failed = report(
            item(Outcome::Failed("offline".into()), Decision::Push),
            1,
            0,
            0,
        );
        let line = error_line(&Ok(failed), "s").unwrap();
        assert!(
            line.contains("claude/s.cast.gz") && line.contains("offline"),
            "{line}"
        );
        assert!(!line.contains('\n'));
        let conflict = report(
            item(
                Outcome::Skipped("conflict, use --overwrite".into()),
                Decision::Conflict,
            ),
            0,
            0,
            1,
        );
        assert!(error_line(&Ok(conflict), "s").unwrap().contains("conflict"));
        let capped = report(
            item(
                Outcome::Skipped("over the 1 GiB limit".into()),
                Decision::Push,
            ),
            0,
            1,
            0,
        );
        assert!(error_line(&Ok(capped), "s").unwrap().contains("1 GiB"));
        let err = error_line(&Err(SyncError::Failed("boom".into())), "s").unwrap();
        assert!(err.contains("boom"), "{err}");
    }
}
