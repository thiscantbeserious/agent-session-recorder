//! Auto push queue, drain, busy lock, status line and listing on the pack engine.

use agr::files::codec;
use agr::sync::testing::queue::{self, enqueue, list, remove, QUEUE_DIR};
use agr::{Config, StorageManager};
use std::path::Path;
use tempfile::TempDir;

#[test]
fn the_queue_dir_is_ignored_by_listing_and_cleanup() {
    let temp = TempDir::new().unwrap();
    let mut config = Config::default();
    config.storage.directory = temp.path().to_string_lossy().to_string();
    let storage = StorageManager::new(config);
    let path = temp.path().join("claude/a.cast.gz");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    codec::atomic_write(&path, |w| Ok(w.write_all(b"text")?)).unwrap();
    queue::enqueue(temp.path(), "claude/a.cast.gz").unwrap();
    let listed = storage.list_sessions(None).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].agent, "claude");
    assert!(agr::storage::orphans::find(temp.path()).unwrap().is_empty());
    assert!(agr::storage::compress::plan(&storage, None)
        .unwrap()
        .files
        .is_empty());
}

mod packs {
    use crate::helpers::pack_engine::TARGET;
    use crate::helpers::pack_engine::{node, write_v1_index, Node, CAST};
    use crate::helpers::pack_repo::{v1_desc, Legacy};
    use agr::sync::testing::{queue, Listing, MemoryStore, PackStore, Repo, Segment, StoreError};
    use agr::sync::{format, Event, Key, Outcome, SyncError};

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

    #[test]
    fn drain_pushes_every_queued_session_and_empties_the_queue() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let paths: Vec<String> = ["a", "b", "c"].iter().map(|x| queued(&n, x)).collect();
        let report = n
            .engine(&store, &key, "w1")
            .drain_auto(&paths[0], &mut |_: &Event<'_>| {})
            .unwrap();
        assert_eq!(report.pushed, 3, "{:?}", report.items);
        assert_eq!(
            Repo::open(&store, &key, "r", TARGET, None)
                .unwrap()
                .view()
                .iter()
                .count(),
            3
        );
        assert_eq!(queue::list(n.dir()), Vec::<String>::new());
    }

    #[test]
    fn a_manual_push_also_pushes_the_queued_session_and_empties_the_queue() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        queued(&n, "a");
        queued(&n, "b");
        let only_a = agr::sync::Options {
            sessions: vec!["claude/a.cast.gz".into()],
            ..crate::helpers::pack_engine::opts()
        };
        let report = n.engine(&store, &key, "w1").push(&only_a).unwrap();
        // the drain runs after the push and its report is dropped, so only a is counted
        assert_eq!(report.pushed, 1, "{:?}", report.items);
        let repo = Repo::open(&store, &key, "r", TARGET, None).unwrap();
        let view = repo.view();
        assert_eq!(view.iter().count(), 2, "both a and b reached the remote");
        assert!(queue::list(n.dir()).is_empty());
    }

    #[test]
    fn drain_picks_up_an_entry_added_between_items() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let first = queued(&n, "a");
        queued(&n, "b");
        let mut added = false;
        let mut hook = |event: &Event<'_>| {
            if matches!(event, Event::Finished { .. }) && !added {
                added = true;
                queued(&n, "late");
            }
        };
        let report = n
            .engine(&store, &key, "w1")
            .drain_auto(&first, &mut hook)
            .unwrap();
        assert!(added);
        assert_eq!(report.pushed, 3, "{:?}", report.items);
        assert_eq!(
            Repo::open(&store, &key, "r", TARGET, None)
                .unwrap()
                .view()
                .iter()
                .count(),
            3
        );
        assert!(queue::list(n.dir()).is_empty());
    }

    /// Store whose second append fails with a plain error, the first one lands.
    struct FailSecond<'a>(&'a MemoryStore, std::cell::Cell<usize>);

    impl PackStore for FailSecond<'_> {
        fn name(&self) -> &'static str {
            "fail-second"
        }
        fn check(&self) -> Result<(), StoreError> {
            self.0.check()
        }
        fn list(&self) -> Result<Listing, StoreError> {
            self.0.list()
        }
        fn read_index(&self, id: &str) -> Result<String, StoreError> {
            self.0.read_index(id)
        }
        fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
            self.0.create(tag, index)
        }
        fn append(
            &self,
            id: &str,
            n: u64,
            e: &str,
            s: &[Segment],
            i: &str,
        ) -> Result<(), StoreError> {
            self.1.set(self.1.get() + 1);
            if self.1.get() == 2 {
                return Err(StoreError::Unreachable("down on purpose".into()));
            }
            self.0.append(id, n, e, s, i)
        }
        fn read(
            &self,
            id: &str,
            offset: u64,
            len: u64,
        ) -> Result<Box<dyn std::io::Read>, StoreError> {
            self.0.read(id, offset, len)
        }
        fn delete(&self, id: &str) -> Result<(), StoreError> {
            self.0.delete(id)
        }
    }

    #[test]
    fn a_failed_push_keeps_its_entry_and_the_others_go() {
        let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
        let old = queued(&n, "old");
        let new = queued(&n, "new");
        let store = FailSecond(&mem, std::cell::Cell::new(0));
        // target 1 byte, every session gets its own pack and its own append
        let engine = agr::sync::Engine::new(&store, &key, &n.storage, "w1", 1);
        let report = engine.drain_auto(&new, &mut |_: &Event<'_>| {}).unwrap();
        assert_eq!((report.pushed, report.failed), (1, 1), "{:?}", report.items);
        let failed = report
            .items
            .iter()
            .find(|i| matches!(i.outcome, Outcome::Failed(_)))
            .map(|i| i.path.clone())
            .unwrap();
        assert!(failed == old || failed == new, "{failed}");
        assert_eq!(queue::list(n.dir()), [failed]);
    }

    #[test]
    fn status_shows_the_queued_count_compact_and_verbose() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        queued(&n, "a");
        queued(&n, "b");
        let report = n.engine(&store, &key, "w1").status(&[]).unwrap();
        let compact = format::compact_status_lines(&report, "gist");
        let full = format::status_lines(&report, "gist");
        for lines in [compact, full] {
            assert!(
                lines.iter().any(|l| l == "2 queued for auto push"),
                "{lines:?}"
            );
        }
        let clean = node();
        let report = clean.engine(&store, &key, "w1").status(&[]).unwrap();
        let lines = format::compact_status_lines(&report, "gist");
        assert!(!lines.iter().any(|l| l.contains("queued")), "{lines:?}");
    }

    /// Several recordings finish at once, each enqueues first and then runs the auto path, like
    /// the real hook and child. Whoever loses the lock race just returns.
    #[test]
    fn concurrent_recordings_all_reach_the_remote_and_the_queue_ends_empty() {
        for round in 0..40 {
            let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
            let names: Vec<String> = (0..4)
                .map(|i| {
                    let file = format!("s{round}_{i}.cast.gz");
                    n.session("claude", &file, &CAST.replace("secret", &file));
                    format!("claude/{file}")
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
            let repo = Repo::open(&store, &key, "reader", TARGET, None).unwrap();
            assert_eq!(repo.view().iter().count(), names.len(), "round {round}");
            assert!(queue::list(n.dir()).is_empty(), "round {round}");
        }
    }

    /// A busy lock is not an error, the entry stays queued for the holder.
    #[cfg(unix)]
    #[test]
    fn a_busy_auto_run_leaves_the_entry_and_stores_no_error() {
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let one = queued(&n, "one");
        let held = format!("{{\"pid\":{},\"started\":\"x\"}}", std::process::id());
        std::fs::write(n.dir().join(".agr-sync.lock"), held).unwrap();
        let result = n
            .engine(&store, &key, "w1")
            .drain_auto(&one, &mut |_: &Event<'_>| {});
        assert!(matches!(result.as_ref().err(), Some(SyncError::Busy(_))));
        assert_eq!(queue::list(n.dir()), [one]);
        assert_eq!(agr::sync::auto::error_line(&result), None);
        assert_eq!(store.appends(), 0);
    }

    /// A lock that cannot be read right now is not a dead one, the holder may be mid write.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_lock_is_busy_and_stays() {
        use std::os::unix::fs::PermissionsExt;
        let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
        let one = queued(&n, "one");
        let lock = n.dir().join(".agr-sync.lock");
        let held = format!("{{\"pid\":{},\"started\":\"x\"}}", std::process::id());
        std::fs::write(&lock, held).unwrap();
        std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_to_string(&lock).is_ok() {
            return; // running as root, permissions do not apply
        }
        let result = n
            .engine(&store, &key, "w1")
            .drain_auto(&one, &mut |_: &Event<'_>| {});
        assert!(matches!(result.as_ref().err(), Some(SyncError::Busy(_))));
        assert!(lock.exists());
        assert_eq!(queue::list(n.dir()), [one]);
    }
}

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
