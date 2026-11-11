//! Auto push queue, drain, busy lock, status line and listing on the pack engine.

use agr::files::codec;
use agr::sync::testing::queue;
use agr::{Config, StorageManager};
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
