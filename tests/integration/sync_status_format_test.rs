//! What `agr sync status` prints for forks, tombstones, other keys and dead bytes, from a real
//! engine status over the in-memory pack store.

use crate::helpers::pack_engine::{node, raw_push, Node, CAST};
use agr::sync::format::{compact_status_lines, status_lines};
use agr::sync::testing::MemoryStore;
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
