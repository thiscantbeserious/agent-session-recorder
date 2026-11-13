//! Several machines on one pack store, the fresh machine guard, forks and writer ids.

use crate::helpers::pack_engine::{merged, node, opts, raw_push, three, A, CAST, TARGET};
use crate::helpers::pack_repo::{v1_desc, Legacy};
use agr::sync::testing::{copy_key, MemoryStore, PackStore, PushItem, Repo};
use agr::sync::Key;

const X: &str = "claude/x.cast.gz";
const NEEDS: &str = "run `agr sync migrate` first";

fn text_of(err: Option<agr::sync::SyncError>) -> String {
    err.map(|e| e.to_string()).unwrap_or_default()
}

#[test]
fn fresh_machine_with_v1_gists_on_remote_requires_migrate() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    let store = Legacy(&mem, vec![v1_desc(&key)]);
    let e = n.engine(&store, &key, "w1");
    let errs = [
        e.push(&opts()).err(),
        e.pull(&opts()).err(),
        e.delete(&[A.to_string()]).err(),
    ];
    for err in errs {
        let text = text_of(err);
        assert!(text.contains(NEEDS), "{text}");
    }
    assert_eq!(mem.appends(), 0);
}

#[test]
fn deleted_local_index_on_machine_a_refuses_the_same_way() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    n.engine(&mem, &key, "w1").push(&opts()).unwrap();
    let _ = std::fs::remove_file(agr::sync::testing::index::path_for(n.dir()));
    let store = Legacy(&mem, vec![v1_desc(&key)]);
    let appends = mem.appends();
    let text = text_of(n.engine(&store, &key, "w1").push(&opts()).err());
    assert!(text.contains(NEEDS), "{text}");
    assert_eq!(mem.appends(), appends);
}

#[test]
fn v1_gist_of_another_key_does_not_block_and_open_for_migrate_skips_the_guard() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    let foreign = Legacy(&mem, vec![v1_desc(&Key::generate())]);
    let push = n.engine(&foreign, &key, "w1").push(&opts());
    assert!(push.is_ok(), "{:?}", push.err());
    let ours = Legacy(&mem, vec![v1_desc(&key)]);
    assert!(Repo::open_for_migrate(&ours, &key, "w1", 1 << 20, None).is_ok());
}

#[test]
fn fork_is_reported_right_after_push() {
    let (b, store, key) = (node(), MemoryStore::new(), Key::generate());
    raw_push(&store, &key, "w0", (X, 1), "v0");
    b.engine(&store, &key, "wb").pull(&opts()).unwrap();
    b.session("claude", "x.cast.gz", &CAST.replace("secret", "from b"));
    // machine A wins the race: its gen 2 lands between the plan and the append of B
    let (rival, hook_key) = (tempfile::tempdir().unwrap(), copy_key(&key));
    let file = rival.path().join("a.raw");
    std::fs::write(&file, "from a").unwrap();
    store.before_append(move |s, _| {
        let item = PushItem {
            path: X.to_string(),
            gen: 2,
            file: file.clone(),
        };
        let mut repo = Repo::open(s, &hook_key, "wa", TARGET, None).unwrap();
        repo.push_batch(&[item]).unwrap();
    });
    let report = b.engine(&store, &key, "wb").push(&opts()).unwrap();
    assert_eq!(report.forks, vec![X], "{:?}", report.items);
}

#[test]
fn new_writer_id_after_state_loss() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    n.session("claude", "a.cast.gz", CAST);
    n.engine(&store, &key, "old").push(&opts()).unwrap();
    n.session("claude", "b.cast.gz", &CAST.replace("secret", "b"));
    n.engine(&store, &key, "new").push(&opts()).unwrap();
    assert_eq!(store.list().unwrap().packs.len(), 2);
    let (at, _) = merged(&store, &key, A);
    assert!(at.is_some(), "the old open pack stays merged");
}
