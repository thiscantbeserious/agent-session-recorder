//! The sync engine on a `Repo` over the in-memory pack store.

use crate::helpers::pack_engine::{node, opts, three};
use agr::sync::testing::{MemoryStore, PackStore};
use agr::sync::Key;

#[test]
fn push_of_three_sessions_is_one_pack() {
    let (n, store, key) = (node(), MemoryStore::new(), Key::generate());
    three(&n);
    let report = n.engine(&store, &key, "w1").push(&opts()).unwrap();
    assert_eq!(store.list().unwrap().packs.len(), 1);
    assert_eq!(report.pushed, 3, "{:?}", report.items);
}
