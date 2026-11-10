//! The `--auto` child run and the error line it leaves in the state file.

use agr::config::state::State;
use agr::sync::auto::{error_line, run, take_error_line};
use agr::sync::testing::{seal_meta, sha256_hex, MemoryBackend, Meta};
use agr::sync::{
    BackendEngine, Decision, Item, Key, Options, Outcome, Report, SyncBackend, SyncError,
};
use agr::{Config, StorageManager};
use std::fs;
use std::path::{Path, PathBuf};

use crate::helpers::sync_net::seal_blob;

/// A gzip session `claude/s.cast.gz` under `dir`.
fn session(dir: &Path) -> PathBuf {
    fs::create_dir_all(dir.join("claude")).unwrap();
    let path = dir.join("claude/s.cast.gz");
    fs::write(&path, b"x").unwrap();
    path
}

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
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.storage.directory = dir.path().to_string_lossy().into_owned();
    let storage = StorageManager::new(cfg);
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
    let engine = BackendEngine {
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

/// Storage with `claude/<name>.cast.gz` files, all pushed once by a manual push.
struct World {
    dir: tempfile::TempDir,
    backend: MemoryBackend,
    key: Key,
    storage: StorageManager,
}

impl World {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::default();
        cfg.storage.directory = dir.path().to_string_lossy().into_owned();
        Self {
            storage: StorageManager::new(cfg),
            backend: MemoryBackend::new(),
            key: Key::generate(),
            dir,
        }
    }

    fn engine(&self) -> BackendEngine<'_> {
        BackendEngine {
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
        let opts = Options {
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
    let index = agr::sync::testing::index::load(w.dir.path()).0;
    let changed = agr::sync::testing::index::maybe_changed(w.dir.path(), &index);
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
fn error_line_is_none_when_busy_because_the_entry_is_queued() {
    assert_eq!(error_line(&Err(SyncError::Busy)), None);
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
