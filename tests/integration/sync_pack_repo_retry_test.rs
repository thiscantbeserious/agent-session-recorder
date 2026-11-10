use crate::helpers::pack_repo::{item, packs_of, repo, tamper, TARGET};
use agr::sync::testing::Index;
use agr::sync::testing::MemoryStore;
use agr::sync::testing::{copy_key, Outcome, Repo};
use agr::sync::testing::{Listing, PackStore, Segment, StoreError};
use agr::sync::Key;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

/// Store that counts `read_index` per id and records the `expected_index` of every append.
struct Spy<'a> {
    inner: &'a MemoryStore,
    reads: RefCell<BTreeMap<String, usize>>,
    /// `expected_index` and the `read_index` calls so far, per append.
    expected: RefCell<Vec<(String, usize)>>,
}

impl<'a> Spy<'a> {
    fn new(inner: &'a MemoryStore) -> Self {
        Self {
            inner,
            reads: RefCell::default(),
            expected: RefCell::default(),
        }
    }

    fn read_index_calls(&self, id: &str) -> usize {
        self.reads.borrow().get(id).copied().unwrap_or(0)
    }
}

impl PackStore for Spy<'_> {
    fn name(&self) -> &'static str {
        "spy"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.inner.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        self.inner.list()
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        *self.reads.borrow_mut().entry(id.to_string()).or_default() += 1;
        self.inner.read_index(id)
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.inner.create(tag, index)
    }
    fn append(
        &self,
        id: &str,
        len: u64,
        expected: &str,
        segs: &[Segment],
        index: &str,
    ) -> Result<(), StoreError> {
        let seen = self.read_index_calls(id);
        self.expected
            .borrow_mut()
            .push((expected.to_string(), seen));
        self.inner.append(id, len, expected, segs, index)
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.inner.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.inner.delete(id)
    }
}

/// Two repos of one writer id on one pack that holds `s` and `t`, both opened before any push.
fn two_on_one_id<'a>(store: &'a dyn PackStore, key: &'a Key) -> (Repo<'a>, Repo<'a>) {
    let dir = tempfile::tempdir().unwrap();
    let mut first = repo(store, key, "w1");
    first
        .push_batch(&[
            item(dir.path(), "s", 1, b"s"),
            item(dir.path(), "t", 1, b"t"),
        ])
        .unwrap();
    (repo(store, key, "w1"), repo(store, key, "w1"))
}

#[test]
fn two_writers_on_one_id_retry_once() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    a.push_batch(&[item(dir.path(), "x", 1, b"x")]).unwrap();
    let second = b.push_batch(&[item(dir.path(), "y", 1, b"y")]);
    assert!(second.is_ok(), "{second:?}");
    let view = repo(&mem, &key, "w1");
    assert!(["s", "t", "x", "y"]
        .iter()
        .all(|p| view.view().get(p).is_some()));
    assert_eq!(packs_of(&mem).len(), 1);
}

#[test]
fn second_moved_gives_the_run_again_message() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (mut a, _) = two_on_one_id(&mem, &key);
    a.push_batch(&[item(dir.path(), "x", 1, b"x")]).unwrap();
    let hostile = copy_key(&key);
    mem.before_append(move |m, id| {
        let info = packs_of(m).into_iter().find(|p| p.id == id).unwrap();
        crate::helpers::pack_repo::tamper(m, &hostile, &info, |_| {});
    });
    let mut b2 = Repo::open(&mem, &key, "w1", TARGET, None).unwrap();
    let err = b2
        .push_batch(&[item(dir.path(), "y", 1, b"y")])
        .unwrap_err();
    assert_eq!(err.to_string(), "changed on the remote, run again");
}

#[test]
fn retry_uses_the_index_from_moved() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let spy = Spy::new(&mem);
    let (mut a, mut b) = two_on_one_id(&spy, &key);
    a.push_batch(&[item(dir.path(), "x", 1, b"x")]).unwrap();
    let id = packs_of(&mem)[0].id.clone();
    let current = mem.read_index(&id).unwrap();
    let pushed = b.push_batch(&[item(dir.path(), "y", 1, b"y")]);
    assert!(pushed.is_ok(), "{pushed:?}");
    let calls = spy.expected.borrow();
    let (moved, retry) = (&calls[calls.len() - 2], &calls[calls.len() - 1]);
    assert_eq!(
        retry.1, moved.1,
        "no read_index between the Moved and the retry"
    );
    assert_eq!(retry.0, current);
}

#[test]
fn tombstones_of_two_writers_on_one_id_both_land() {
    let (mem, key) = (MemoryStore::new(), Key::generate());
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    a.tombstone("s").unwrap();
    assert!(b.tombstone("t").is_ok());
    let view = repo(&mem, &key, "w1");
    let dead = |p: &str| view.view().get(p).unwrap().is_deleted();
    assert!(dead("s") && dead("t"));
}

#[test]
fn tombstone_landed_after_retry_reports_its_real_outcome() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    a.push_batch(&[item(dir.path(), "s", 2, b"s2")]).unwrap();
    let report = b.tombstone("s").unwrap();
    let pack = packs_of(&mem)[0].id.clone();
    // planned at gen 2, replanned to 3 behind the moved index, and it landed
    assert_eq!(
        report.items,
        vec![("s".to_string(), 3, Outcome::Pushed { pack })]
    );
}

#[test]
fn retry_replans_gens_from_the_moved_index() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    a.push_batch(&[item(dir.path(), "s", 2, b"s2")]).unwrap();
    b.tombstone("s").unwrap();
    let view = repo(&mem, &key, "w1");
    let s = view.view().get("s").unwrap();
    assert_eq!(s.gen, 3);
    assert!(s.is_deleted());
}

#[test]
fn retry_drops_a_session_the_moved_index_is_ahead_of() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    a.push_batch(&[item(dir.path(), "s", 3, b"s3")]).unwrap();
    b.push_batch(&[item(dir.path(), "s", 2, b"s2")]).unwrap();
    let id = packs_of(&mem)[0].id.clone();
    let info = packs_of(&mem).remove(0);
    let gens: Vec<u64> = crate::helpers::pack_repo::index_of(&mem, &key, &info)
        .entries
        .iter()
        .filter(|e| e.path == "s")
        .map(|e| e.gen)
        .collect();
    assert_eq!(gens, vec![1, 3], "{id}");
}

#[test]
fn no_empty_pack_after_moved_on_a_sealed_pack() {
    let (mem, key, dir) = (
        MemoryStore::with_slots(1),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    repo(&mem, &key, "w1")
        .push_batch(&[item(dir.path(), "a", 1, b"x")])
        .unwrap();
    let mut b = repo(&mem, &key, "w1");
    crate::helpers::pack_repo::tamper(&mem, &key, &packs_of(&mem)[0], |i| i.sealed = true);
    let before = packs_of(&mem).len();
    b.push_batch(&[item(dir.path(), "b", 1, b"y")]).unwrap();
    assert_eq!(packs_of(&mem).len(), before + 1);
}

#[test]
fn retry_decides_the_seal_from_the_moved_index() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    let big = vec![5u8; TARGET as usize - 1000];
    a.push_batch(&[item(dir.path(), "x", 1, &big)]).unwrap();
    b.push_batch(&[item(dir.path(), "y", 1, &vec![6u8; 2000])])
        .unwrap();
    let info = packs_of(&mem).remove(0);
    assert!(crate::helpers::pack_repo::index_of(&mem, &key, &info).sealed);
}

#[test]
fn list_error_in_retry_is_returned_not_ignored() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    a.push_batch(&[item(dir.path(), "x", 1, b"x")]).unwrap();
    let (once, hostile) = (AtomicBool::new(true), copy_key(&key));
    mem.before_append(move |m, id| {
        if once.swap(false, Ordering::SeqCst) {
            let info = packs_of(m).into_iter().find(|p| p.id == id).unwrap();
            crate::helpers::pack_repo::tamper(m, &hostile, &info, |_| {});
            m.fail_next_list();
        }
    });
    let res = b.push_batch(&[item(dir.path(), "y", 1, b"y")]);
    assert!(res.is_err(), "{res:?}");
}

#[test]
fn replanned_chunk_in_a_new_pack_does_not_seal_it_early() {
    let (mem, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let mut first = repo(&mem, &key, "w1");
    let near = vec![1u8; TARGET as usize - 2000];
    first
        .push_batch(&[item(dir.path(), "x", 1, &near)])
        .unwrap();
    let (once, same) = (AtomicBool::new(true), copy_key(&key));
    mem.before_append(move |m, id| {
        if once.swap(false, Ordering::SeqCst) {
            let info = packs_of(m).into_iter().find(|p| p.id == id).unwrap();
            crate::helpers::pack_repo::tamper(m, &same, &info, |i| i.sealed = true);
        }
    });
    first
        .push_batch(&[item(dir.path(), "y", 1, &vec![2u8; 3000])])
        .unwrap();
    let newest = packs_of(&mem).pop().unwrap();
    assert!(!crate::helpers::pack_repo::index_of(&mem, &key, &newest).sealed);
}

#[test]
fn replanned_append_respects_the_index_cap() {
    const CAP: usize = 768 * 1024;
    let (store, key, dir) = (
        MemoryStore::new(),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let open = || Repo::open(&store, &key, "w1", 512 * 1024 * 1024, None).unwrap();
    open()
        .push_batch(&[item(dir.path(), "s", 1, b"s")])
        .unwrap();
    let mut late = open();
    // the same writer fills the index up to just below the cap, `late` plans against the old one
    let info = packs_of(&store).remove(0);
    tamper(&store, &key, &info, |i| {
        let mut e = i.entries[0].clone();
        let size = |i: &_| agr::sync::testing::seal_index(&key, i).unwrap().len();
        let mut add = |i: &mut Index, n: usize| {
            for _ in 0..n {
                e.path = format!("filler-{:06}-{}", i.entries.len(), "x".repeat(60));
                i.entries.push(e.clone());
            }
        };
        // Measure one entry, jump close to the cap, then refine: a few seals, not thousands.
        let base = size(i);
        add(i, 1);
        let per = size(i).saturating_sub(base).max(1);
        add(i, (CAP - 3000).saturating_sub(size(i)) / per);
        while size(i) < CAP - 3000 {
            add(i, 1);
        }
    });
    let more: Vec<_> = (0..60)
        .map(|k| {
            item(
                dir.path(),
                &format!("late-session-recording-{k:05}"),
                1,
                b"x",
            )
        })
        .collect();
    late.push_batch(&more).unwrap();
    let longest = store.index_texts().iter().map(String::len).max().unwrap();
    assert!(longest <= CAP, "{longest}");
}

#[test]
fn replanned_append_respects_the_file_cap() {
    let (store, key, dir) = (
        MemoryStore::with_slots(2),
        Key::generate(),
        tempfile::tempdir().unwrap(),
    );
    let open = || Repo::open(&store, &key, "w1", TARGET, None).unwrap();
    open()
        .push_batch(&[item(dir.path(), "s", 1, b"s")])
        .unwrap();
    let mut late = open();
    // another tab of the writer takes the second slot while `late` plans against one file
    open()
        .push_batch(&[item(dir.path(), "t", 1, b"t")])
        .unwrap();
    late.push_batch(&[item(dir.path(), "u", 1, b"u")]).unwrap();
    let old = packs_of(&store).remove(0);
    let in_old = crate::helpers::pack_repo::index_of(&store, &key, &old)
        .entries
        .len();
    assert!(in_old <= 2, "{in_old} sessions in a pack of 2 slots");
}
