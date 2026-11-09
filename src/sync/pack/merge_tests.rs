use crate::sync::pack::index::{Entry, Index, IndexError};
use crate::sync::pack::merge::{merge, PackIn, Rejected, Top};

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

#[test]
fn tombstone_on_top_is_deleted_with_its_gen() {
    let p = pack(
        "p",
        "P",
        20,
        vec![entry("a", 4, "x", 10, false), entry("a", 5, "x", 10, true)],
    );
    let view = merge(&[p]);
    assert_eq!(view.get("a").unwrap().top, Top::Deleted { gen: 5 });
}

fn rejected_pack(id: &str, err: IndexError) -> PackIn {
    PackIn {
        id: id.to_string(),
        tag: id.to_uppercase(),
        index: Err(err),
    }
}

#[test]
fn other_key_pack_is_reported_never_merged() {
    let x = rejected_pack("bad", IndexError::OtherKey);
    let ok = pack("p", "P", 10, vec![entry("a", 1, "x", 10, false)]);
    let view = merge(&[x, ok]);
    assert_eq!(view.rejected, vec![("bad".to_string(), Rejected::OtherKey)]);
    assert!(view.get("x").is_none());
}

#[test]
fn unknown_version_pack_is_reported() {
    let view = merge(&[rejected_pack("bad", IndexError::UnknownVersion(4))]);
    assert_eq!(view.rejected[0].1, Rejected::UnknownVersion(4));
}

#[test]
fn index_of_other_pack_is_reported() {
    let view = merge(&[rejected_pack("bad", IndexError::OtherPack)]);
    assert_eq!(view.rejected[0].1, Rejected::OtherPack);
}

#[test]
fn dead_bytes_per_pack() {
    let p = pack(
        "p",
        "P",
        90,
        vec![entry("a", 2, "x", 40, false), entry("a", 4, "y", 50, false)],
    );
    let view = merge(&[p]);
    assert_eq!(view.dead_bytes["p"], 40);
}

#[test]
fn dead_bytes_count_a_top_gen_tombstone_as_live() {
    let p = pack("p", "P", 30, vec![entry("a", 5, "x", 30, true)]);
    assert_eq!(merge(&[p]).dead_bytes["p"], 0);
}

#[test]
fn duplicated_pack_is_merged_and_reported() {
    let rows = || vec![entry("x", 1, "h", 10, false), entry("t", 2, "h", 5, true)];
    let (a, b) = (pack("a", "T", 15, rows()), pack("b", "T", 15, rows()));
    let view = merge(&[a, b]);
    assert_eq!(view.duplicates, vec![("a".to_string(), "b".to_string())]);
    assert_eq!(view.get("x").unwrap().places.len(), 2);
    assert!(view.get("t").unwrap().is_deleted());
}

#[test]
fn both_ids_of_a_duplicate_are_frozen() {
    let rows = || vec![entry("x", 1, "h", 10, false)];
    let (a, b) = (pack("a", "T", 10, rows()), pack("b", "T", 10, rows()));
    let c = pack("c", "U", 10, rows());
    let view = merge(&[a, b, c]);
    assert!(view.is_frozen("a") && view.is_frozen("b"));
    assert!(!view.is_frozen("c"));
}

#[test]
fn keeper_is_the_place_in_the_highest_pack_id() {
    let a = pack("a", "A", 10, vec![entry("x", 1, "h", 10, false)]);
    let b = pack("b", "B", 10, vec![entry("x", 1, "h", 10, false)]);
    let view = merge(&[a, b]);
    assert_eq!(view.get("x").unwrap().keeper().pack, "b");
    assert_eq!(view.dead_bytes["a"], 10);
    assert_eq!(view.dead_bytes["b"], 0);
}

#[test]
fn older_index_of_same_pack_is_not_accepted() {
    let new = pack("a", "A", 200, vec![entry("x", 4, "h", 200, false)]);
    let old = pack("a", "A", 100, vec![entry("x", 3, "g", 100, false)]);
    let view = merge(&[new, old]);
    assert_eq!(view.pack("a").unwrap().len, 200);
    assert_eq!(view.get("x").unwrap().gen, 4);
    let (new, old) = (
        pack("a", "A", 200, vec![entry("x", 4, "h", 200, false)]),
        pack("a", "A", 100, vec![entry("x", 3, "g", 100, false)]),
    );
    let view = merge(&[old, new]);
    assert_eq!(view.pack("a").unwrap().len, 200);
    assert_eq!(view.get("x").unwrap().gen, 4);
}

#[test]
fn pack_order_does_not_change_the_view() {
    let rows = || vec![entry("x", 1, "h", 10, false), entry("y", 2, "k", 5, true)];
    let mut all = vec![
        pack("a", "T", 15, rows()),
        pack("b", "T", 15, rows()),
        pack("c", "C", 10, vec![entry("x", 1, "z", 10, false)]),
        rejected_pack("d", IndexError::OtherKey),
        rejected_pack("e", IndexError::OtherPack),
        pack("f", "F", 30, vec![entry("x", 0, "h", 30, false)]),
    ];
    let want = merge(&all);
    for shift in 1..all.len() {
        all.rotate_left(shift);
        assert_eq!(merge(&all), want);
        all.reverse();
        assert_eq!(merge(&all), want);
    }
}

#[test]
fn fork_losers_are_live_not_dead() {
    let a = pack("a", "A", 10, vec![entry("x", 4, "y", 10, false)]);
    let b = pack("b", "B", 10, vec![entry("x", 4, "z", 10, false)]);
    let view = merge(&[a, b]);
    assert_eq!(view.forks, vec!["x"]);
    assert_eq!(view.dead_bytes.get("a").copied().unwrap_or(0), 0);
    assert_eq!(view.dead_bytes.get("b").copied().unwrap_or(0), 0);
}

#[test]
fn sealed_index_wins_a_tie_in_any_order() {
    let open = || pack("a", "A", 10, vec![entry("x", 1, "h", 10, false)]);
    let sealed = || {
        let mut p = open();
        p.index.as_mut().unwrap().sealed = true;
        p
    };
    assert!(merge(&[open(), sealed()]).pack("a").unwrap().sealed);
    assert!(merge(&[sealed(), open()]).pack("a").unwrap().sealed);
}
