use super::tests::{item, packs_of, repo, TARGET};
use super::Repo;
use crate::sync::crypto::Key;
use crate::sync::store::{Listing, PackStore, Segment, StoreError};
use crate::sync::testing::MemoryStore;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Read;

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
    let (mut a, mut b) = two_on_one_id(&mem, &key);
    a.push_batch(&[item(dir.path(), "x", 1, b"x")]).unwrap();
    let id = packs_of(&mem)[0].id.clone();
    let hostile = Hostile {
        inner: &mem,
        id,
        key: &key,
    };
    let mut b2 = Repo::open(&hostile, &key, "w1", TARGET, None).unwrap();
    let err = b2
        .push_batch(&[item(dir.path(), "y", 1, b"y")])
        .unwrap_err();
    assert_eq!(err.to_string(), "changed on the remote, run again");
    let _ = &mut b;
}

/// Every append finds the pack changed again, as if a third writer won each time.
struct Hostile<'a> {
    inner: &'a MemoryStore,
    id: String,
    key: &'a Key,
}

impl PackStore for Hostile<'_> {
    fn name(&self) -> &'static str {
        "hostile"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.inner.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        self.inner.list()
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
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
        if id == self.id {
            let info = packs_of(self.inner)
                .into_iter()
                .find(|p| p.id == id)
                .unwrap();
            super::tests::tamper(self.inner, self.key, &info, |_| {});
        }
        self.inner.append(id, len, expected, segs, index)
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        self.inner.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.inner.delete(id)
    }
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
