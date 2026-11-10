//! Behaviour every `PackStore` must show, run per store through `contract_tests!`.

use std::io::{self, Read, Write};

use super::{PackStore, Segment, StoreError};

fn put(data: &'static [u8]) -> impl Fn(&mut dyn Write) -> io::Result<()> {
    move |w| w.write_all(data)
}

fn seg<'a>(len: u64, write: &'a dyn Fn(&mut dyn Write) -> io::Result<()>) -> Segment<'a> {
    Segment { len, write }
}

fn len_of(store: &dyn PackStore, id: &str) -> u64 {
    let listing = store.list().expect("list");
    listing
        .packs
        .iter()
        .find(|p| p.id == id)
        .expect("listed")
        .len
}

/// A pack holding `abc` behind index `i1`.
fn pack_with_abc(store: &dyn PackStore) -> String {
    let id = store.create("tag", "i0").expect("create");
    let w = put(b"abc");
    store
        .append(&id, 0, "i0", &[seg(3, &w)], "i1")
        .expect("append");
    id
}

pub fn append_refuses_wrong_expected_len(store: &dyn PackStore) {
    let id = store.create("tag", "i0").expect("create");
    let w = put(b"abc");
    let err = store.append(&id, 10, "i0", &[seg(3, &w)], "i1").err();
    assert!(matches!(err, Some(StoreError::Moved(_))), "{err:?}");
}

pub fn append_refuses_other_expected_index(store: &dyn PackStore) {
    let id = store.create("tag", "i0").expect("create");
    let err = store.append(&id, 0, "x0", &[], "i1").err();
    assert!(matches!(err, Some(StoreError::Moved(_))), "{err:?}");
}

pub fn moved_carries_the_current_index(store: &dyn PackStore) {
    let id = pack_with_abc(store);
    let err = store.append(&id, 0, "i0", &[], "i2").err();
    assert_eq!(err, Some(StoreError::Moved("i1".to_string())));
}

pub fn zero_segment_append_replaces_only_the_index(store: &dyn PackStore) {
    let id = pack_with_abc(store);
    store.append(&id, 3, "i1", &[], "i2").expect("zero append");
    assert_eq!(store.read_index(&id).expect("index"), "i2");
    assert_eq!(len_of(store, &id), 3);
}

pub fn append_is_all_or_nothing(store: &dyn PackStore) {
    let id = pack_with_abc(store);
    let bad = |w: &mut dyn Write| {
        w.write_all(b"xx")?;
        Err(io::Error::other("boom"))
    };
    let res = store.append(&id, 3, "i1", &[seg(2, &bad)], "i2");
    assert!(res.is_err(), "{res:?}");
    assert_eq!(len_of(store, &id), 3);
    assert_eq!(store.read_index(&id).expect("index"), "i1");
}

pub fn segment_len_mismatch_is_rejected(store: &dyn PackStore) {
    let id = pack_with_abc(store);
    let w = put(b"defg");
    let res = store.append(&id, 3, "i1", &[seg(3, &w)], "i2");
    assert!(matches!(res, Err(StoreError::Other(_))), "{res:?}");
    assert_eq!(len_of(store, &id), 3);
    assert_eq!(store.read_index(&id).expect("index"), "i1");
}

pub fn read_returns_the_exact_range(store: &dyn PackStore) {
    let id = pack_with_abc(store);
    let w = put(b"defg");
    store
        .append(&id, 3, "i1", &[seg(4, &w)], "i2")
        .expect("append");
    let mut bytes = Vec::new();
    store
        .read(&id, 2, 4)
        .expect("read")
        .read_to_end(&mut bytes)
        .expect("bytes");
    assert_eq!(bytes, b"abcdefg"[2..6]);
}

pub fn read_past_end_is_reported_the_same_by_every_store(store: &dyn PackStore) {
    let id = pack_with_abc(store);
    for (offset, len) in [(0, 4), (3, 1), (10, 1), (u64::MAX, 2), (1, u64::MAX)] {
        let err = store.read(&id, offset, len).err();
        assert!(
            matches!(err, Some(StoreError::NotFound)),
            "{offset}+{len}: {err:?}"
        );
    }
}

pub fn create_then_list_shows_len_zero_and_tag(store: &dyn PackStore) {
    let id = store.create("tag-x", "i0").expect("create");
    let listing = store.list().expect("list");
    let info = listing.packs.iter().find(|p| p.id == id).expect("listed");
    assert_eq!((info.len, info.tag.as_str()), (0, "tag-x"));
    assert!(listing.legacy.is_empty());
}

pub fn delete_then_read_index_is_not_found(store: &dyn PackStore) {
    let id = store.create("tag", "i0").expect("create");
    store.delete(&id).expect("delete");
    let err = store.read_index(&id).err();
    assert!(matches!(err, Some(StoreError::NotFound)), "{err:?}");
}

/// Runs every check on one store.
pub fn contract(store: &dyn PackStore) {
    append_refuses_wrong_expected_len(store);
    append_refuses_other_expected_index(store);
    moved_carries_the_current_index(store);
    zero_segment_append_replaces_only_the_index(store);
    append_is_all_or_nothing(store);
    segment_len_mismatch_is_rejected(store);
    read_returns_the_exact_range(store);
    read_past_end_is_reported_the_same_by_every_store(store);
    create_then_list_shows_len_zero_and_tag(store);
    delete_then_read_index_is_not_found(store);
}

/// One `#[test]` per check for a store made by `$make`, which returns `(store, guard)`.
macro_rules! contract_tests {
    ($make:expr) => {
        contract_tests!(@each $make;
            append_refuses_wrong_expected_len,
            append_refuses_other_expected_index,
            moved_carries_the_current_index,
            zero_segment_append_replaces_only_the_index,
            append_is_all_or_nothing,
            segment_len_mismatch_is_rejected,
            read_returns_the_exact_range,
            read_past_end_is_reported_the_same_by_every_store,
            create_then_list_shows_len_zero_and_tag,
            delete_then_read_index_is_not_found
        );
        #[test]
        fn passes_the_contract() {
            let (store, _guard) = ($make)();
            $crate::sync::store::contract::contract(&store);
        }
    };
    (@each $make:expr; $($name:ident),*) => {
        $(
            #[test]
            fn $name() {
                let (store, _guard) = ($make)();
                $crate::sync::store::contract::$name(&store);
            }
        )*
    };
}
