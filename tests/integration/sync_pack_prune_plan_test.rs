//! The pure prune plan, over synthetic merged views (tests live here, not under src).

use agr::sync::testing::{merge, prune_plan, Entry, Index, PackIn};

fn entry(path: &str, gen: u64, sha: &str, length: u64, deleted: bool) -> Entry {
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

fn pack(id: &str, len: u64, sealed: bool, entries: Vec<Entry>) -> PackIn {
    pack_by(id, "w", len, sealed, entries)
}

fn pack_by(id: &str, writer: &str, len: u64, sealed: bool, entries: Vec<Entry>) -> PackIn {
    let tag = id.to_uppercase();
    let index = Index {
        v: 3,
        pack: tag.clone(),
        writer: writer.to_string(),
        sealed,
        len,
        entries,
    };
    PackIn {
        id: id.to_string(),
        tag,
        index: Ok(index),
    }
}

#[test]
fn fully_dead_sealed_pack_is_deleted() {
    let a = pack("a", 10, true, vec![entry("x", 1, "s1", 10, false)]);
    let b = pack("b", 10, true, vec![entry("x", 2, "s2", 10, false)]);
    let plan = prune_plan(&merge(&[a, b]));
    assert_eq!(plan.delete, vec!["a".to_string()]);
}

/// Pack `id` holds 70 dead bytes and 30 live ones, its replaced session lives on in pack `z`.
fn with_dead(id: &str) -> Vec<PackIn> {
    let old = pack(
        id,
        100,
        true,
        vec![
            entry("x", 1, "old", 70, false),
            entry("k", 1, "k1", 30, false),
        ],
    );
    let new = pack("z", 70, false, vec![entry("x", 2, "new", 70, false)]);
    vec![old, new]
}

#[test]
fn dead_share_20_percent_is_repacked_19_is_kept() {
    // pack a: 100 bytes, 80 live (x) and 20 dead (y was replaced), pack b: 81 live and 19 dead
    let live = |n: &str, sha: &str, len: u64| entry(n, 1, sha, len, false);
    let a = pack(
        "a",
        100,
        true,
        vec![live("x", "x1", 80), live("y", "y1", 20)],
    );
    let b = pack(
        "b",
        100,
        true,
        vec![live("p", "p1", 81), live("q", "q1", 19)],
    );
    let newer = pack(
        "z",
        40,
        false,
        vec![
            entry("y", 2, "y2", 20, false),
            entry("q", 2, "q2", 19, false),
        ],
    );
    let plan = prune_plan(&merge(&[a, b, newer]));
    assert_eq!(plan.repack, vec!["a".to_string()]);
    assert!(!plan.repack.contains(&"b".to_string()));
}

#[test]
fn open_packs_are_never_planned() {
    let mine = pack_by("a", "w", 10, false, vec![entry("x", 1, "s1", 10, false)]);
    let theirs = pack_by(
        "b",
        "other",
        10,
        false,
        vec![entry("x", 2, "s2", 10, false)],
    );
    let plan = prune_plan(&merge(&[mine, theirs]));
    assert!(plan.is_empty(), "{plan:?}");
}

#[test]
fn tombstones_are_copied() {
    // a: x at gen 1 (dead) and the tombstone of x at gen 2 (live)
    let a = pack(
        "a",
        60,
        true,
        vec![entry("x", 1, "s1", 60, false), entry("x", 2, "", 0, true)],
    );
    let plan = prune_plan(&merge(&[a]));
    assert_eq!(plan.copy_tombstones, 1);
}

#[test]
fn live_tombstone_pack_is_not_fully_dead() {
    let a = pack(
        "a",
        40,
        true,
        vec![entry("x", 1, "s1", 40, false), entry("x", 2, "", 0, true)],
    );
    let id = "a".to_string();
    let plan = prune_plan(&merge(&[a]));
    assert!(plan.delete.is_empty(), "{plan:?}");
    assert_eq!(plan.repack, vec![id]);
}

#[test]
fn two_places_tie_break_is_deterministic() {
    let a = || pack("a", 10, true, vec![entry("x", 1, "s", 10, false)]);
    let b = || pack("b", 10, true, vec![entry("x", 1, "s", 10, false)]);
    // the tags differ, so these are two packs with one session, not copies of one pack
    let ab = prune_plan(&merge(&[a(), b()]));
    let ba = prune_plan(&merge(&[b(), a()]));
    assert_eq!(ab.delete, ba.delete);
    assert_eq!(ab.delete, vec!["a".to_string()]);
}

#[test]
fn prune_never_touches_unreadable_packs() {
    let other_key = PackIn {
        id: "k".into(),
        tag: "K".into(),
        index: Err(agr::sync::testing::IndexError::OtherKey),
    };
    let unknown = PackIn {
        id: "u".into(),
        tag: "U".into(),
        index: Err(agr::sync::testing::IndexError::UnknownVersion(9)),
    };
    let plan = prune_plan(&merge(&[other_key, unknown]));
    assert!(!plan.touches("k") && !plan.touches("u"));
}

#[test]
fn dry_run_reports_bytes_to_move_and_free() {
    let plan = prune_plan(&merge(&with_dead("a")));
    assert_eq!(plan.repack, vec!["a".to_string()]);
    assert_eq!((plan.move_bytes, plan.free_bytes), (30, 70));
}
