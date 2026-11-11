//! Auto push queue, drain, busy lock, status line and listing against the in-memory backend

use agr::files::codec;
use agr::sync::testing::queue;
use agr::sync::testing::MemoryBackend;
use agr::sync::{format, BackendEngine, Event, Key, Options, SyncError};
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

    fn engine(&self) -> BackendEngine<'_> {
        BackendEngine {
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
    assert!(matches!(result.as_ref().err(), Some(SyncError::Busy(_))));
    assert_eq!(queue::list(w.dir()), ["claude/a.cast.gz"]);
    assert_eq!(agr::sync::auto::error_line(&result), None);
}

/// A lock that cannot be read right now is not a dead one, the holder may be mid write.
#[cfg(unix)]
#[test]
fn an_unreadable_lock_is_busy_and_stays() {
    use std::os::unix::fs::PermissionsExt;
    let w = world();
    w.queued("claude/a.cast.gz");
    let lock = w.dir().join(".agr-sync.lock");
    let held = format!("{{\"pid\":{},\"started\":\"x\"}}", std::process::id());
    std::fs::write(&lock, held).unwrap();
    std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read_to_string(&lock).is_ok() {
        return; // running as root, permissions do not apply
    }
    let result = w.drain("claude/a.cast.gz");
    assert!(matches!(result.as_ref().err(), Some(SyncError::Busy(_))));
    assert!(lock.exists());
    assert_eq!(queue::list(w.dir()), ["claude/a.cast.gz"]);
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
                    let engine = BackendEngine {
                        backend: &w.backend,
                        key: &w.key,
                        storage: &w.storage,
                    };
                    let result = engine.drain_auto(n, &mut |_: &Event<'_>| {});
                    assert!(
                        matches!(result, Ok(_) | Err(SyncError::Busy(_))),
                        "{result:?}"
                    );
                });
            }
        });
        assert_eq!(w.backend.len(), names.len(), "round {round}");
        assert!(queue::list(w.dir()).is_empty(), "round {round}");
    }
}

mod packs {
    use crate::helpers::pack_engine::{node, write_v1_index, Node, CAST};
    use crate::helpers::pack_repo::{v1_desc, Legacy};
    use agr::sync::testing::{queue, MemoryStore, Repo};
    use agr::sync::{Event, Key, SyncError};

    const NEEDS: &str = "run `agr sync migrate` first";

    fn queued(n: &Node, name: &str) -> String {
        let path = format!("claude/{name}.cast.gz");
        n.session(
            "claude",
            &format!("{name}.cast.gz"),
            &CAST.replace("secret", name),
        );
        queue::enqueue(n.dir(), &path).unwrap();
        path
    }

    #[test]
    fn ten_queued_sessions_are_one_append() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let paths: Vec<String> = (0..10).map(|i| queued(&n, &format!("s{i}"))).collect();
        let report = n
            .engine(&store, &key, "w1")
            .drain_auto(&paths[0], &mut |_: &Event<'_>| {})
            .unwrap();
        assert_eq!(store.appends(), 1);
        assert_eq!(report.pushed, 10, "{:?}", report.items);
        assert!(queue::list(n.dir()).is_empty());
    }

    #[test]
    fn burst_during_drain_lands_in_the_next_round_together() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let first = queued(&n, "first");
        let mut added = false;
        let mut hook = |event: &Event<'_>| {
            if matches!(event, Event::Finished { .. }) && !added {
                added = true;
                for i in 0..5 {
                    queued(&n, &format!("late{i}"));
                }
            }
        };
        let report = n
            .engine(&store, &key, "w1")
            .drain_auto(&first, &mut hook)
            .unwrap();
        assert!(added);
        assert_eq!(store.appends(), 2);
        assert_eq!(report.pushed, 6, "{:?}", report.items);
        assert!(queue::list(n.dir()).is_empty());
    }

    #[test]
    fn single_recording_is_pushed_without_waiting() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let one = queued(&n, "one");
        n.engine(&store, &key, "w1")
            .drain_auto(&one, &mut |_: &Event<'_>| {})
            .unwrap();
        assert_eq!(store.appends(), 1);
    }

    #[test]
    fn auto_push_on_v1_index_does_nothing_and_hints_migrate() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let one = queued(&n, "one");
        write_v1_index(&n);
        let err = n
            .engine(&store, &key, "w1")
            .drain_auto(&one, &mut |_: &Event<'_>| {})
            .err();
        let text = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(text.contains(NEEDS), "{text}");
        assert_eq!(store.appends(), 0);
        assert_eq!(queue::list(n.dir()), [one]);
    }

    #[test]
    fn auto_push_on_fresh_machine_with_v1_gists_does_nothing() {
        let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
        let one = queued(&n, "one");
        let store = Legacy(&mem, vec![v1_desc(&key)]);
        let err = n
            .engine(&store, &key, "w1")
            .drain_auto(&one, &mut |_: &Event<'_>| {})
            .err();
        let text = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(text.contains(NEEDS), "{text}");
        assert_eq!(mem.appends(), 0);
        assert_eq!(queue::list(n.dir()).len(), 1);
    }

    /// The append lands, the index save fails, the entry stays queued. The next run must
    /// recognise its own landed entry from the merged view and push nothing.
    #[cfg(unix)]
    #[test]
    fn a_failed_index_save_after_a_landed_append_does_not_push_twice() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let one = queued(&n, "one");
        let index = n.dir().join(".agr-sync.json.tmp");
        std::fs::create_dir(&index).unwrap();
        let first = n
            .engine(&store, &key, "w1")
            .drain_auto(&one, &mut |_: &Event<'_>| {});
        assert!(first.is_err(), "{first:?}");
        assert_eq!(store.appends(), 1);
        assert_eq!(queue::list(n.dir()), std::slice::from_ref(&one));
        std::fs::remove_dir(&index).unwrap();
        let second = n
            .engine(&store, &key, "w1")
            .drain_auto(&one, &mut |_: &Event<'_>| {})
            .unwrap();
        assert_eq!(store.appends(), 1, "{:?}", second.items);
        assert!(queue::list(n.dir()).is_empty());
        let repo = Repo::open(
            &store,
            &key,
            "reader",
            crate::helpers::pack_engine::TARGET,
            None,
        )
        .unwrap();
        assert_eq!(repo.view().get(&one).map(|m| m.gen), Some(1));
    }

    /// Eight recordings finish at once, each enqueues first and then runs the auto path.
    #[test]
    fn n_parallel_recordings_pushed_exactly_once() {
        for round in 0..30 {
            let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
            let names: Vec<String> = (0..8)
                .map(|i| {
                    let name = format!("claude/r{round}p{i}.cast.gz");
                    n.session(
                        "claude",
                        &format!("r{round}p{i}.cast.gz"),
                        &CAST.replace("secret", &name),
                    );
                    name
                })
                .collect();
            std::thread::scope(|s| {
                for name in &names {
                    s.spawn(|| {
                        queue::enqueue(n.dir(), name).unwrap();
                        let result = n
                            .engine(&store, &key, "w1")
                            .drain_auto(name, &mut |_: &Event<'_>| {});
                        assert!(
                            matches!(result, Ok(_) | Err(SyncError::Busy(_))),
                            "{result:?}"
                        );
                    });
                }
            });
            let repo = Repo::open(
                &store,
                &key,
                "reader",
                crate::helpers::pack_engine::TARGET,
                None,
            )
            .unwrap();
            assert_eq!(repo.view().iter().count(), 8);
            for name in &names {
                assert_eq!(repo.view().get(name).map(|m| m.gen), Some(1), "{name}");
            }
            assert!(queue::list(n.dir()).is_empty());
        }
    }
}
