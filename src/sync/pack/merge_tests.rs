use crate::sync::pack::index::{Entry, Index};
use crate::sync::pack::merge::{merge, PackIn};

pub(super) fn entry(path: &str, gen: u64, sha: &str, length: u64, deleted: bool) -> Entry {
    Entry {
        path: path.to_string(),
        gen,
        sha256: sha.to_string(),
        size: length,
        offset: 0,
        length,
        deleted,
        at: "2026-11-09T10:00:00Z".to_string(),
    }
}

pub(super) fn pack(id: &str, tag: &str, len: u64, entries: Vec<Entry>) -> PackIn {
    let index = Index {
        v: 3,
        pack: tag.to_string(),
        writer: "w".to_string(),
        sealed: false,
        len,
        entries,
    };
    PackIn {
        id: id.to_string(),
        tag: tag.to_string(),
        index: Ok(index),
    }
}

#[test]
fn highest_gen_wins() {
    let p = pack(
        "p",
        "P",
        90,
        vec![entry("a", 2, "x", 40, false), entry("a", 4, "y", 50, false)],
    );
    let view = merge(&[p]);
    assert_eq!(view.get("a").unwrap().gen, 4);
}

#[test]
fn same_hash_twice_is_one_session_two_places() {
    let a = pack("p1", "P1", 10, vec![entry("a", 1, "x", 10, false)]);
    let b = pack("p2", "P2", 10, vec![entry("a", 1, "x", 10, false)]);
    let view = merge(&[a, b]);
    assert_eq!(view.get("a").unwrap().places.len(), 2);
}

#[test]
fn different_hash_same_top_gen_is_fork() {
    let a = pack("p1", "P1", 10, vec![entry("a", 4, "x", 10, false)]);
    let b = pack("p2", "P2", 10, vec![entry("a", 4, "y", 10, false)]);
    let view = merge(&[a, b]);
    assert_eq!(view.forks, vec!["a"]);
}

#[test]
fn fork_below_top_gen_is_ignored() {
    let a = pack(
        "p1",
        "P1",
        20,
        vec![entry("a", 3, "x", 10, false), entry("a", 4, "y", 10, false)],
    );
    let b = pack("p2", "P2", 10, vec![entry("a", 3, "z", 10, false)]);
    let view = merge(&[a, b]);
    assert!(view.forks.is_empty());
}
