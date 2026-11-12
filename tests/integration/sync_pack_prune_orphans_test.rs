//! `--seal-orphans` on the memory store, time is injected (tests live here, not under src).

use crate::helpers::pack_repo::{index_of, item, packs_of, repo, TARGET};
use agr::sync::testing::{
    prune_run, seal_index, seal_plan, Listing, MemoryStore, PackStore, PruneEnv, PruneReport, Repo,
    Segment, StoreError, SEAL_BUDGET,
};
use agr::sync::Key;

const DAY: i64 = 86_400;
/// 2026-11-12 00:00:00 UTC.
const NOW: i64 = 1_794_441_600;
const MIB: u64 = 1024 * 1024;

fn at(days_ago: i64) -> String {
    let t = chrono::DateTime::from_timestamp(NOW - days_ago * DAY, 0).unwrap();
    t.to_rfc3339()
}

fn run(store: &MemoryStore, key: &Key, dry: bool, budget: u64) -> PruneReport {
    let scratch = tempfile::tempdir().unwrap();
    let env = PruneEnv {
        store,
        key,
        writer: "w9",
        target: TARGET,
        hint: None,
        scratch: scratch.path(),
        dry_run: dry,
        seal_orphans: true,
        now: NOW,
        seal_budget: budget,
        pace: &|| {},
    };
    prune_run(&env).unwrap()
}

/// One open pack of `writer` with one session, listed as idle for `days`.
fn orphan(store: &MemoryStore, key: &Key, writer: &str, days: i64) -> String {
    let dir = tempfile::tempdir().unwrap();
    let name = format!("s-{writer}");
    repo(store, key, writer)
        .push_batch(&[item(dir.path(), &name, 1, b"data")])
        .unwrap();
    let id = packs_of(store).into_iter().last().unwrap().id;
    store.set_updated_at(&id, &at(days));
    id
}

fn sealed(store: &MemoryStore, key: &Key, id: &str) -> bool {
    let info = packs_of(store).into_iter().find(|p| p.id == id).unwrap();
    index_of(store, key, &info).sealed
}

fn len_of(store: &MemoryStore, id: &str) -> u64 {
    packs_of(store)
        .into_iter()
        .find(|p| p.id == id)
        .unwrap()
        .len
}

#[test]
fn seal_orphans_seals_idle_open_packs_of_other_writers() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let (old, young) = (
        orphan(&store, &key, "w2", 31),
        orphan(&store, &key, "w3", 29),
    );
    let own = orphan(&store, &key, "w9", 31);
    let len_before = len_of(&store, &old);
    run(&store, &key, false, SEAL_BUDGET);
    assert!(sealed(&store, &key, &old));
    assert_eq!(len_of(&store, &old), len_before);
    assert!(!sealed(&store, &key, &young));
    assert!(!sealed(&store, &key, &own));
}

#[test]
fn sealed_orphan_owner_starts_a_fresh_pack() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let dir = tempfile::tempdir().unwrap();
    let id = orphan(&store, &key, "w2", 31);
    let mut owner = repo(&store, &key, "w2");
    run(&store, &key, false, SEAL_BUDGET);
    assert!(sealed(&store, &key, &id));
    owner
        .push_batch(&[item(dir.path(), "next", 1, b"more")])
        .unwrap();
    assert_eq!(store.list().unwrap().packs.len(), 2);
}

#[test]
fn seal_orphans_loses_the_race_to_the_owner_and_drops_the_seal() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let id = orphan(&store, &key, "w2", 31);
    let (attempts, k) = (
        std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        agr::sync::testing::copy_key(&key),
    );
    let count = attempts.clone();
    store.before_append(move |s, _| {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = tempfile::tempdir().unwrap();
        let mut owner = Repo::open(s, &k, "w2", TARGET, None).unwrap();
        owner
            .push_batch(&[item(dir.path(), "late", 1, b"late")])
            .unwrap();
    });
    let report = run(&store, &key, false, SEAL_BUDGET);
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!sealed(&store, &key, &id));
    let view = Repo::open(&store, &key, "r", TARGET, None).unwrap();
    assert!(view.view().get("late").is_some());
    let text = report.lines.join("\n");
    assert!(text.contains("changed during the seal, skipped"), "{text}");
}

/// An open pack of `writer` that holds `len` bytes of data and an index without entries.
fn heavy(store: &MemoryStore, key: &Key, writer: &str, len: u64) -> String {
    let id = orphan(store, key, writer, 31);
    let info = packs_of(store).into_iter().find(|p| p.id == id).unwrap();
    let mut idx = index_of(store, key, &info);
    let text = store.read_index(&id).unwrap();
    idx.len += len;
    let zeros = |w: &mut dyn std::io::Write| w.write_all(&vec![0u8; len as usize]);
    let seg = [Segment { len, write: &zeros }];
    let next = seal_index(key, &idx).unwrap();
    store.append(&id, info.len, &text, &seg, &next).unwrap();
    id
}

#[test]
fn seal_orphans_dry_run_prints_download_size() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    heavy(&store, &key, "w2", 48 * MIB - 1000);
    heavy(&store, &key, "w3", 48 * MIB - 1000);
    let before = store.appends();
    let out = run(&store, &key, true, SEAL_BUDGET).lines.join("\n");
    assert!(out.contains("2 packs to seal, 96 MiB to download"), "{out}");
    assert_eq!(store.appends(), before);
}

#[test]
fn a_real_run_stops_at_the_download_budget() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let ids: Vec<String> = ["w2", "w3", "w4"]
        .iter()
        .map(|w| heavy(&store, &key, w, MIB))
        .collect();
    let out = run(&store, &key, false, 2 * MIB + 1000).lines.join("\n");
    assert!(out.contains("1 left, run again"), "{out}");
    let done = ids.iter().filter(|id| sealed(&store, &key, id)).count();
    assert_eq!(done, 2);
}

#[test]
fn seal_orphans_never_touches_a_duplicated_pack() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let a = orphan(&store, &key, "w2", 31);
    let info = packs_of(&store).into_iter().find(|p| p.id == a).unwrap();
    let copy = store
        .create(&info.tag, &store.read_index(&a).unwrap())
        .unwrap();
    store.set_updated_at(&copy, &at(31));
    let r = Repo::open(&store, &key, "w9", TARGET, None).unwrap();
    let plan = seal_plan(r.view(), &packs_of(&store), "w9", NOW, SEAL_BUDGET);
    assert!(
        !plan.seal.contains(&a) && !plan.seal.contains(&copy),
        "{plan:?}"
    );
}

/// Reads fine until the second listing of a run, then every index text is garbage, so the index
/// opens for the plan and fails at the seal.
struct Rots<'a>(&'a MemoryStore, std::sync::atomic::AtomicUsize);

impl PackStore for Rots<'_> {
    fn name(&self) -> &'static str {
        "rots"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.0.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.0.list()
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        match self.1.load(std::sync::atomic::Ordering::SeqCst) {
            0..=1 => self.0.read_index(id),
            _ => Ok("not an index".to_string()),
        }
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.0.create(tag, index)
    }
    fn append(&self, id: &str, n: u64, e: &str, s: &[Segment], i: &str) -> Result<(), StoreError> {
        self.0.append(id, n, e, s, i)
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn std::io::Read>, StoreError> {
        self.0.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.0.delete(id)
    }
}

#[test]
fn an_unreadable_index_is_reported_as_unreadable_not_as_a_race() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let id = orphan(&store, &key, "w2", 31);
    let rots = Rots(&store, Default::default());
    let scratch = tempfile::tempdir().unwrap();
    let env = PruneEnv {
        store: &rots,
        key: &key,
        writer: "w9",
        target: TARGET,
        hint: None,
        scratch: scratch.path(),
        dry_run: false,
        seal_orphans: true,
        now: NOW,
        seal_budget: SEAL_BUDGET,
        pace: &|| {},
    };
    let text = prune_run(&env).unwrap().lines.join("\n");
    assert!(
        text.contains(&format!("{id}: index unreadable, skipped")),
        "{text}"
    );
    assert!(!text.contains("changed during the seal"), "{text}");
}

#[test]
fn an_orphan_larger_than_the_whole_budget_is_skipped_and_named() {
    let (store, key) = (MemoryStore::new(), Key::generate());
    let big = heavy(&store, &key, "w2", 3 * MIB);
    let small = orphan(&store, &key, "w3", 31);
    let out = run(&store, &key, false, 2 * MIB).lines.join("\n");
    assert!(!sealed(&store, &key, &big), "{out}");
    assert!(sealed(&store, &key, &small), "{out}");
    assert!(
        out.contains(&format!("{big}: larger than the seal budget, skipped")),
        "{out}"
    );
    assert!(!out.contains("left, run again"), "{out}");
}
