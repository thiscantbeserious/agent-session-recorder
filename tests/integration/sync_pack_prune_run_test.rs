//! Prune execution on the memory store (tests live here, not under src).

use crate::helpers::pack_engine::{node, opts, CAST};
use crate::helpers::pack_repo::{index_of, item, packs_of, repo, FILL, TARGET};
use agr::sync::testing::{
    copy_key, prune_run, sha256_hex, MemoryStore, PruneEnv, PruneError, PruneReport, Repo,
    RepoError, SEAL_BUDGET,
};
use agr::sync::Key;
use std::path::Path;

pub fn prune(store: &MemoryStore, key: &Key, writer: &str, dry: bool) -> PruneReport {
    try_prune(store, key, writer, dry).unwrap()
}

fn try_prune(
    store: &MemoryStore,
    key: &Key,
    writer: &str,
    dry: bool,
) -> Result<PruneReport, PruneError> {
    let scratch = tempfile::tempdir().unwrap();
    let env = PruneEnv {
        store,
        key,
        writer,
        target: TARGET,
        hint: None,
        scratch: scratch.path(),
        dry_run: dry,
        seal_orphans: false,
        now: 0,
        seal_budget: SEAL_BUDGET,
        pace: &|| {},
    };
    prune_run(&env)
}

const BIG_A: usize = 40_000;
const BIG_B: usize = 30_000;

/// Pack one seals with `a` (40000) and `b` (30000), then `a` is replaced at gen 2, so the pack
/// is 57 percent dead and `b` only lives there. Returns the plain bytes of b.
fn scenario(store: &MemoryStore, key: &Key, dir: &Path) -> Vec<u8> {
    let b = vec![7u8; BIG_B];
    let mut w1 = repo(store, key, "w1");
    w1.push_batch(&[item(dir, "a", 1, &vec![1u8; BIG_A]), item(dir, "b", 1, &b)])
        .unwrap();
    w1.push_batch(&[item(dir, "a", 2, b"tiny")]).unwrap();
    b
}

fn fixture() -> (MemoryStore, Key, tempfile::TempDir) {
    (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    )
}

fn places(store: &MemoryStore, key: &Key, path: &str) -> usize {
    let r = Repo::open(store, key, "reader", TARGET, None).unwrap();
    r.view().get(path).map_or(0, |m| m.places.len())
}

#[test]
fn copies_keep_path_gen_and_sha() {
    let (store, key, dir) = fixture();
    let b = scenario(&store, &key, dir.path());
    prune(&store, &key, "w9", false);
    let mine = packs_of(&store)
        .into_iter()
        .find(|p| index_of(&store, &key, p).writer == "w9");
    let idx = index_of(&store, &key, &mine.unwrap());
    let c = idx.entries.iter().find(|e| e.path == "b").unwrap();
    assert_eq!(
        (c.path.as_str(), c.gen, c.sha256.as_str()),
        ("b", 1, sha256_hex(&b).as_str())
    );
}

#[test]
fn old_pack_deleted_only_after_copies_are_visible() {
    let (store, key, dir) = fixture();
    scenario(&store, &key, dir.path());
    store.after_append(|s, _| s.fail_list());
    let res = try_prune(&store, &key, "w9", false);
    assert!(res.is_err() || res.unwrap().failed.is_some());
    assert_eq!(store.deletes(), 0);
}

#[test]
fn two_prunes_at_once_lose_nothing() {
    let (store, key, dir) = fixture();
    let b = scenario(&store, &key, dir.path());
    let before = Repo::open(&store, &key, "r", TARGET, None)
        .unwrap()
        .view()
        .iter()
        .count();
    let k = copy_key(&key);
    store.before_append(move |s, _| {
        prune(s, &k, "w8", false);
    });
    prune(&store, &key, "w9", false);
    assert!(store.deletes() >= 1);
    let r = Repo::open(&store, &key, "r", TARGET, None).unwrap();
    assert_eq!(r.view().iter().count(), before);
    let out = dir.path().join("out");
    r.read_session("b", &out).unwrap();
    assert_eq!(std::fs::read(out).unwrap(), b);
}

#[test]
fn concurrent_prunes_never_delete_the_last_place() {
    let (store, key, dir) = fixture();
    scenario(&store, &key, dir.path());
    let k = copy_key(&key);
    store.after_append(move |s, _| {
        prune(s, &k, "w8", false);
    });
    prune(&store, &key, "w9", false);
    assert!(store.deletes() >= 1);
    for path in ["a", "b"] {
        assert!(places(&store, &key, path) >= 1, "{path}");
    }
}

#[test]
fn pull_during_prune_retries_with_the_copy() {
    let (store, key, dir) = fixture();
    let b = scenario(&store, &key, dir.path());
    let reader = Repo::open(&store, &key, "r", TARGET, None).unwrap();
    assert_eq!(prune(&store, &key, "w9", false).deleted, 1);
    let out = dir.path().join("pulled");
    reader.read_session("b", &out).unwrap();
    assert_eq!(sha256_hex(&std::fs::read(out).unwrap()), sha256_hex(&b));
}

#[test]
fn partial_index_view_aborts_push_delete_and_prune() {
    let (store, key, dir) = fixture();
    scenario(&store, &key, dir.path());
    let id = packs_of(&store)[0].id.clone();
    store.fail_read_index(&id);
    let (appends, deletes) = (store.appends(), store.deletes());
    let res = try_prune(&store, &key, "w9", false);
    assert!(
        matches!(res, Err(PruneError::Open(RepoError::PartialView(_)))),
        "{res:?}"
    );
    let n = node();
    n.session("claude", "a.cast.gz", CAST);
    let engine = n.engine(&store, &key, "w1");
    assert!(engine.push(&opts()).is_err());
    assert!(engine.delete(&["claude/a.cast.gz".to_string()]).is_err());
    assert_eq!(
        (store.appends() - appends, store.deletes() - deletes),
        (0, 0)
    );
}

const BIG_LIVE: usize = FILL;

/// The prune writers each own an open pack with a lower id than the old pack: `z` and `y` are
/// pushed first (mem-1, mem-2), then the old pack (mem-3) holds `a` (20000, replaced later) and
/// `b` (FILL live, enough to fill a copy pack). A prune reuses its own open pack for the copy.
fn low_copy_ids(store: &MemoryStore, key: &Key, dir: &Path) {
    for (writer, name) in [("w9", "z"), ("w8", "y")] {
        repo(store, key, writer)
            .push_batch(&[item(dir, name, 1, b"x")])
            .unwrap();
    }
    let mut w1 = repo(store, key, "w1");
    let items = [
        item(dir, "a", 1, &vec![1u8; 20_000]),
        item(dir, "b", 1, &vec![7u8; BIG_LIVE]),
    ];
    w1.push_batch(&items).unwrap();
    w1.push_batch(&[item(dir, "a", 2, b"tiny")]).unwrap();
}

#[test]
fn a_full_copy_pack_is_not_sealed_by_prune() {
    let (store, key, dir) = fixture();
    low_copy_ids(&store, &key, dir.path());
    prune(&store, &key, "w9", false);
    let copy = packs_of(&store).into_iter().find(|p| {
        index_of(&store, &key, p)
            .entries
            .iter()
            .any(|e| e.path == "b")
            && p.id != "mem-3"
    });
    let idx = index_of(&store, &key, &copy.expect("a copy pack"));
    assert!(!idx.sealed);
}

#[test]
fn two_prunes_with_full_low_id_copy_packs_keep_every_place() {
    let (store, key, dir) = fixture();
    low_copy_ids(&store, &key, dir.path());
    let k = copy_key(&key);
    store.after_append(move |s, _| {
        prune(s, &k, "w8", false);
    });
    prune(&store, &key, "w9", false);
    store.after_append(|_, _| {});
    prune(&store, &key, "w9", false);
    prune(&store, &key, "w8", false);
    for path in ["a", "b", "z", "y"] {
        assert!(places(&store, &key, path) >= 1, "{path}");
    }
}

#[test]
fn a_fork_tombstone_in_a_repacked_pack_is_carried_over_once() {
    let (store, key, dir) = fixture();
    let d = dir.path();
    repo(&store, &key, "w1")
        .push_batch(&[item(d, "a", 1, b"x")])
        .unwrap();
    repo(&store, &key, "w2")
        .push_batch(&[item(d, "a", 1, b"y")])
        .unwrap();
    let mut w3 = repo(&store, &key, "w3");
    w3.copy_tombstones(&[("a".to_string(), 1)]).unwrap();
    let both = [
        item(d, "c", 1, &vec![1u8; BIG_A]),
        item(d, "e", 1, &vec![7u8; BIG_B]),
    ];
    w3.push_batch(&both).unwrap();
    repo(&store, &key, "w4")
        .push_batch(&[item(d, "c", 2, b"tiny")])
        .unwrap();
    prune(&store, &key, "w9", false);
    let second = prune(&store, &key, "w9", true);
    let line = second.lines.join("\n");
    assert!(line.contains("0 to repack"), "{line}");
}
