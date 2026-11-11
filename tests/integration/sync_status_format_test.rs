//! What `agr sync status` prints for forks, tombstones, other keys and dead bytes, from a real
//! engine status over the in-memory pack store.

use crate::helpers::gist_rig::Rig;
use crate::helpers::pack_engine::{node, opts, raw_push, three, Node, CAST, TARGET};
use agr::sync::format::{compact_status_lines, status_lines};
use agr::sync::testing::{MemoryStore, PackStore, Repo};
use agr::sync::{Key, Report};

const X: &str = "claude/x.cast.gz";

fn status(n: &Node, store: &MemoryStore, key: &Key) -> Report {
    n.engine(store, key, "w9").status(&[]).unwrap()
}

fn compact(report: &Report) -> String {
    compact_status_lines(report, "memory").join("\n")
}

fn full(report: &Report) -> String {
    status_lines(report, "memory").join("\n")
}

/// Two writers put different content at the same gen, the local file is a third version.
fn forked() -> (Node, MemoryStore, Key) {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &key, "w1", (X, 4), "one");
    raw_push(&store, &key, "w2", (X, 4), "two");
    n.session("claude", "x.cast.gz", CAST);
    (n, store, key)
}

#[test]
fn status_shows_fork_with_next_step() {
    let (n, store, key) = forked();
    let report = status(&n, &store, &key);
    for out in [compact(&report), full(&report)] {
        assert!(out.contains("agr sync push --overwrite"), "{out}");
    }
    let verbose = full(&report);
    assert!(verbose.contains("claude/x.cast.gz fork"), "{verbose}");
    assert!(!verbose.contains("duplicate"), "{verbose}");
}

fn other_key_packs() -> (Node, MemoryStore, Key, Vec<String>) {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &Key::generate(), "w1", ("a/one.cast.gz", 1), "one");
    raw_push(&store, &Key::generate(), "w2", ("a/two.cast.gz", 1), "two");
    let ids = store
        .list()
        .unwrap()
        .packs
        .into_iter()
        .map(|p| p.id)
        .collect();
    (n, store, key, ids)
}

#[test]
fn status_shows_other_key_per_pack() {
    let (n, store, key, ids) = other_key_packs();
    let report = status(&n, &store, &key);
    for out in [compact(&report), full(&report)] {
        for id in &ids {
            let line = out.lines().find(|l| l.contains(id.as_str()));
            let has = line.is_some_and(|l| l.contains("other key"));
            assert!(has, "pack {id} has no other key line in:\n{out}");
        }
    }
}

#[test]
fn status_shows_deleted_on_remote_with_restore_hint() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &key, "w1", (X, 4), "one");
    let mut repo = Repo::open(&store, &key, "w1", TARGET, None).unwrap();
    repo.tombstone(X).unwrap();
    n.session("claude", "x.cast.gz", CAST);
    let report = status(&n, &store, &key);
    let verbose = full(&report);
    assert!(
        verbose.contains("claude/x.cast.gz deleted on the remote"),
        "{verbose}"
    );
    assert!(verbose.contains("agr sync push --overwrite"), "{verbose}");
    let short = compact(&report);
    assert!(short.contains("deleted on the remote"), "{short}");
    assert!(short.contains("agr sync push --overwrite"), "{short}");
}

/// One session replaced at gen 2 leaves `dead` bytes behind, `kept` bytes stay live.
fn dead_share(dead: usize, kept: usize) -> String {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    let text = |len: usize, c: char| c.to_string().repeat(len);
    raw_push(&store, &key, "w1", ("a/old.cast.gz", 1), &text(dead, 'd'));
    raw_push(&store, &key, "w1", ("a/old.cast.gz", 2), &text(10, 'n'));
    raw_push(&store, &key, "w1", ("a/keep.cast.gz", 1), &text(kept, 'k'));
    compact(&status(&n, &store, &key))
}

#[test]
fn prune_hint_at_21_percent_dead_not_at_19() {
    let (out21, out19) = (dead_share(4200, 15_800), dead_share(3800, 16_200));
    assert!(out21.contains("dead data"), "{out21}");
    assert!(!out21.contains("agr sync prune"), "{out21}");
    assert!(!out19.contains("dead data"), "{out19}");
}

#[test]
fn status_aborts_on_partial_index_view() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &key, "w1", (X, 1), "one");
    raw_push(&store, &key, "w2", ("claude/y.cast.gz", 1), "two");
    let id = store.list().unwrap().packs[0].id.clone();
    store.fail_read_index(&id);
    let err = n.engine(&store, &key, "w9").status(&[]).err();
    let text = err.map(|e| e.to_string()).unwrap_or_default();
    assert!(text.contains("could not read"), "{text:?}");
}

fn api_calls(rig: &Rig) -> usize {
    rig.remote
        .calls()
        .iter()
        .filter(|c| c.starts_with("gh api") && !c.contains("gists?"))
        .count()
}

#[test]
fn status_needs_no_clone() {
    let (n, key, rig) = (node(), Key::generate(), Rig::new());
    three(&n);
    n.engine(&rig.store(), &key, "w1").push(&opts()).unwrap();
    // a second machine: fresh store, cold index cache
    let cache = tempfile::tempdir().unwrap();
    let cold = rig.store().with_cache_dir(cache.path().to_path_buf());
    rig.remote.clear_calls();
    n.engine(&cold, &key, "w1").status(&[]).unwrap();
    let calls = rig.remote.calls();
    assert!(!calls.iter().any(|c| c.contains("clone")), "{calls:?}");
    assert_eq!(
        api_calls(&rig),
        1,
        "one index read per pack, cold: {calls:?}"
    );
    rig.remote.clear_calls();
    let warm = rig.store().with_cache_dir(cache.path().to_path_buf());
    n.engine(&warm, &key, "w1").status(&[]).unwrap();
    assert_eq!(
        api_calls(&rig),
        0,
        "sealed packs hit the cache: {:?}",
        rig.remote.calls()
    );
}

#[test]
fn backend_line_counts_no_opaque_pack_rows() {
    use agr::sync::{Decision, Item, Outcome};
    let row = |path: &str| Item {
        path: path.into(),
        decision: Decision::Pull,
        outcome: Outcome::Nothing,
        size: 1,
        on_remote: true,
        on_local: false,
        local_hash: None,
        remote_hash: None,
        remote_size: 1,
    };
    let report = Report {
        items: vec![row("a/x.cast.gz"), row("remote:abc")],
        ..Report::default()
    };
    for out in [compact(&report), full(&report)] {
        assert!(out.contains("1 remote session(s)"), "{out}");
    }
}
