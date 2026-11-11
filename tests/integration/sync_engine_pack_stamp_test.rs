//! The stamp stored by a refresh belongs to the bytes that were hashed.

use crate::helpers::pack_engine::{index_of, node, opts, raw_push};
use agr::sync::testing::{index, Listing, MemoryStore, PackStore, Segment, StoreError};
use agr::sync::Key;
use std::io::Read;

/// Runs `hook` once on the first `read`, the pull of an earlier path while a later one waits.
struct OnRead<'a>(
    &'a MemoryStore,
    std::cell::RefCell<Option<Box<dyn FnOnce() + 'a>>>,
);

impl PackStore for OnRead<'_> {
    fn name(&self) -> &'static str {
        "on-read"
    }
    fn check(&self) -> Result<(), StoreError> {
        self.0.check()
    }
    fn list(&self) -> Result<Listing, StoreError> {
        self.0.list()
    }
    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        self.0.read_index(id)
    }
    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        self.0.create(tag, index)
    }
    fn append(&self, id: &str, n: u64, e: &str, s: &[Segment], i: &str) -> Result<(), StoreError> {
        self.0.append(id, n, e, s, i)
    }
    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        if let Some(hook) = self.1.borrow_mut().take() {
            hook();
        }
        self.0.read(id, offset, len)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.0.delete(id)
    }
}

#[test]
fn a_file_edited_between_scan_and_refresh_is_still_listed_as_maybe_changed() {
    let (n, mem, key) = (node(), MemoryStore::new(), Key::generate());
    let z = "claude/z.cast.gz";
    raw_push(&mem, &key, "w1", ("claude/a.cast.gz", 1), "one");
    raw_push(&mem, &key, "w1", (z, 1), "same");
    n.session("claude", "keep.cast.gz", "x");
    let path = n.dir().join(z);
    std::fs::write(&path, "same").unwrap();
    let edit = || std::fs::write(&path, "edited after the scan, longer").unwrap();
    let store = OnRead(&mem, std::cell::RefCell::new(Some(Box::new(edit))));
    let r = n.engine(&store, &key, "w2").pull(&opts()).unwrap();
    assert_eq!(r.failed, 0, "{:?}", r.items);
    assert!(index_of(&n).entries.contains_key(z), "the refresh ran");
    assert_eq!(index::maybe_changed(n.dir(), &index_of(&n)), [z]);
}
