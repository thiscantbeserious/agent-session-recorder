//! Auto push queue, drain, busy lock, status line and listing against the in-memory backend

use agr::files::codec;
use agr::sync::testing::queue;
use agr::sync::testing::MemoryBackend;
use agr::sync::{auto, format, Engine, Event, Key, Options, SyncError};
use agr::{Config, StorageManager};
use std::path::Path;
use tempfile::TempDir;

struct World {
    temp: TempDir,
    storage: StorageManager,
    backend: MemoryBackend,
    key: Key,
}

fn world() -> World {
    let temp = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.directory = temp.path().to_string_lossy().to_string();
    World {
        storage: StorageManager::new(config),
        backend: MemoryBackend::new(),
        key: Key::generate(),
        temp,
    }
}

fn write(dir: &Path, name: &str, text: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    codec::atomic_write(&path, |w| Ok(w.write_all(text.as_bytes())?)).unwrap();
}

impl World {
    fn dir(&self) -> &Path {
        self.temp.path()
    }

    fn engine(&self) -> Engine<'_> {
        Engine {
            backend: &self.backend,
            key: &self.key,
            storage: &self.storage,
        }
    }

    fn queued(&self, name: &str) {
        write(self.dir(), name, &format!("text of {name}"));
        queue::enqueue(self.dir(), name).unwrap();
    }

    fn drain(&self, session: &str) -> Result<agr::sync::Report, SyncError> {
        self.engine().drain_auto(session, &mut |_: &Event<'_>| {})
    }
}

#[test]
fn drain_pushes_every_queued_session_and_empties_the_queue() {
    let w = world();
    for name in ["claude/a.cast.gz", "claude/b.cast.gz", "codex/c.cast.gz"] {
        w.queued(name);
    }
    let report = w.drain("claude/a.cast.gz").unwrap();
    assert_eq!(report.pushed, 3);
    assert_eq!(w.backend.len(), 3);
    assert_eq!(queue::list(w.dir()), Vec::<String>::new());
}

#[test]
fn drain_picks_up_an_entry_added_between_items() {
    let w = world();
    w.queued("claude/a.cast.gz");
    w.queued("claude/b.cast.gz");
    let mut added = false;
    let mut hook = |event: &Event<'_>| {
        if matches!(event, Event::Finished { .. }) && !added {
            added = true;
            w.queued("claude/late.cast.gz");
        }
    };
    let report = w
        .engine()
        .drain_auto("claude/a.cast.gz", &mut hook)
        .unwrap();
    assert!(added);
    assert_eq!(report.pushed, 3, "{:?}", report.items);
    assert_eq!(w.backend.len(), 3);
    assert!(queue::list(w.dir()).is_empty());
}

#[test]
fn a_failed_push_keeps_its_entry_and_the_others_go() {
    let w = world();
    write(w.dir(), "claude/old.cast.gz", "v1");
    let opts = Options {
        sessions: vec!["claude/old.cast.gz".into()],
        ..Options::default()
    };
    assert_eq!(w.engine().push(&opts).unwrap().pushed, 1);
    write(w.dir(), "claude/old.cast.gz", "v2 changed");
    w.backend.fail_on("mem-1");
    queue::enqueue(w.dir(), "claude/old.cast.gz").unwrap();
    w.queued("claude/new.cast.gz");
    let report = w.drain("claude/new.cast.gz").unwrap();
    assert_eq!((report.pushed, report.failed), (1, 1), "{:?}", report.items);
    assert_eq!(queue::list(w.dir()), ["claude/old.cast.gz"]);
}

#[cfg(unix)]
#[test]
fn a_busy_auto_run_leaves_the_entry_and_stores_no_error() {
    let w = world();
    w.queued("claude/a.cast.gz");
    let held = format!("{{\"pid\":{},\"started\":\"x\"}}", std::process::id());
    std::fs::write(w.dir().join(".agr-sync.lock"), held).unwrap();
    let result = w.drain("claude/a.cast.gz");
    assert_eq!(result.as_ref().err(), Some(&SyncError::Busy));
    assert_eq!(queue::list(w.dir()), ["claude/a.cast.gz"]);
    assert_eq!(auto::error_line(&result), None);
}

#[test]
fn a_manual_push_removes_the_entries_of_what_it_pushed() {
    let w = world();
    w.queued("claude/a.cast.gz");
    w.queued("claude/b.cast.gz");
    let opts = Options {
        sessions: vec!["claude/a.cast.gz".into()],
        ..Options::default()
    };
    assert_eq!(w.engine().push(&opts).unwrap().pushed, 1);
    assert_eq!(queue::list(w.dir()), ["claude/b.cast.gz"]);
}

#[test]
fn status_shows_the_queued_count_compact_and_verbose() {
    let w = world();
    w.queued("claude/a.cast.gz");
    w.queued("claude/b.cast.gz");
    let report = w.engine().status(&[]).unwrap();
    let compact = format::compact_status_lines(&report, "gist");
    let full = format::status_lines(&report, "gist");
    for lines in [compact, full] {
        assert!(
            lines.iter().any(|l| l == "2 queued for auto push"),
            "{lines:?}"
        );
    }
    let clean = world().engine().status(&[]).unwrap();
    let lines = format::compact_status_lines(&clean, "gist");
    assert!(!lines.iter().any(|l| l.contains("queued")), "{lines:?}");
}

#[test]
fn the_queue_dir_is_ignored_by_listing_and_cleanup() {
    let w = world();
    w.queued("claude/a.cast.gz");
    let listed = w.storage.list_sessions(None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].agent, "claude");
    assert!(agr::storage::orphans::find(w.dir()).unwrap().is_empty());
    assert!(agr::storage::compress::plan(&w.storage, None)
        .unwrap()
        .files
        .is_empty());
}

/// Several recordings finish at once, each enqueues first and then runs the auto path, like
/// the real hook and child. Whoever loses the lock race just returns.
#[test]
fn concurrent_recordings_all_reach_the_remote_and_the_queue_ends_empty() {
    let started = std::time::Instant::now();
    for round in 0..40 {
        let w = world();
        let names: Vec<String> = (0..4)
            .map(|i| format!("claude/s{round}_{i}.cast.gz"))
            .collect();
        for n in &names {
            write(w.dir(), n, &format!("text of {n}"));
        }
        std::thread::scope(|s| {
            for n in &names {
                s.spawn(|| {
                    queue::enqueue(w.dir(), n).unwrap();
                    let engine = Engine {
                        backend: &w.backend,
                        key: &w.key,
                        storage: &w.storage,
                    };
                    let result = engine.drain_auto(n, &mut |_: &Event<'_>| {});
                    assert!(matches!(result, Ok(_) | Err(SyncError::Busy)), "{result:?}");
                });
            }
        });
        assert_eq!(w.backend.len(), names.len(), "round {round}");
        assert!(queue::list(w.dir()).is_empty(), "round {round}");
    }
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
}
