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
