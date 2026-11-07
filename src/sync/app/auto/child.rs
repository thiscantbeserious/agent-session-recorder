//! The `--auto` child run and the error line it leaves in the state file.

use crate::config::state::State;
use crate::sync::engine::run::{Engine, Options, Outcome, Report, SyncError};

/// One line for the state file after an `--auto` run, `None` when it went fine or a
/// concurrent push is already running.
pub fn error_line(result: &Result<Report, SyncError>) -> Option<String> {
    let report = match result {
        Err(SyncError::Busy) => return None,
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

/// The `--auto` run, pushes only the one session, never pulls or deletes, a conflict is
/// skipped (no overwrite) and reported.
pub fn run(engine: &Engine<'_>, session: &str) -> Result<Report, SyncError> {
    engine.push(&Options {
        sessions: vec![session.to_string()],
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
        assert!(error_line(&Ok(report)).unwrap().contains("conflict"));
        assert_eq!(fs::read(&path).unwrap(), b"changed locally");
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
        let line = error_line(&Err(SyncError::RateLimited { left: 4 })).unwrap();
        assert_eq!(
            line,
            "auto push failed: GitHub rate limit reached, sync stopped, 4 left, please run again later"
        );
    }

    #[test]
    fn error_line_is_none_for_success() {
        let ok = report(item(Outcome::Done(1), Decision::Push), 0, 0, 0);
        assert_eq!(error_line(&Ok(ok)), None);
    }

    #[test]
    fn error_line_names_the_skipped_session_when_busy() {
        assert!(error_line(&Err(SyncError::Busy)).is_some());
    }

    #[test]
    fn error_line_names_failures_conflicts_and_skips_on_one_line() {
        let failed = report(
            item(Outcome::Failed("offline".into()), Decision::Push),
            1,
            0,
            0,
        );
        let line = error_line(&Ok(failed)).unwrap();
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
        assert!(error_line(&Ok(conflict)).unwrap().contains("conflict"));
        let capped = report(
            item(
                Outcome::Skipped("over the 1 GiB limit".into()),
                Decision::Push,
            ),
            0,
            1,
            0,
        );
        assert!(error_line(&Ok(capped)).unwrap().contains("1 GiB"));
        let err = error_line(&Err(SyncError::Failed("boom".into()))).unwrap();
        assert!(err.contains("boom"), "{err}");
    }
}
