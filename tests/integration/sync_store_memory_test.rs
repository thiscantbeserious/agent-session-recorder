//! `MemoryStore` limits.

use agr::sync::testing::{MemoryStore, PackStore, Segment};
use std::io::Write;

#[test]
fn memory_fits_false_when_slots_run_out() {
    let store = MemoryStore::with_slots(2);
    let id = store.create("t", "i0").unwrap();
    let w = |w: &mut dyn Write| w.write_all(b"a");
    let seg = || Segment { len: 1, write: &w };
    store.append(&id, 0, "i0", &[seg()], "i1").unwrap();
    store.append(&id, 1, "i1", &[seg()], "i2").unwrap();
    let info = store.list().unwrap().packs.remove(0);
    assert!(!store.fits(&info, &[1]));
}

#[test]
fn read_with_overflowing_range_is_an_error() {
    let store = MemoryStore::new();
    let id = store.create("t", "i0").unwrap();
    assert!(store.read(&id, u64::MAX, 2).is_err());
}
