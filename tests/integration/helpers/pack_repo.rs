//! Shared pieces of the pack repo tests.

use agr::sync::testing::{open_index, seal_index, Index, PackInfo, PackStore, PushItem, Repo};
use agr::sync::Key;
use std::path::Path;

pub const TARGET: u64 = 64 * 1024;

/// A session file of `plain` bytes in `dir`.
pub fn item(dir: &Path, path: &str, gen: u64, plain: &[u8]) -> PushItem {
    let file = dir.join(format!("{path}.{gen}"));
    std::fs::write(&file, plain).unwrap();
    PushItem {
        path: path.to_string(),
        gen,
        file,
    }
}

pub fn repo<'a>(store: &'a dyn PackStore, key: &'a Key, writer: &str) -> Repo<'a> {
    Repo::open(store, key, writer, TARGET, None).unwrap()
}

pub fn packs_of(store: &dyn PackStore) -> Vec<PackInfo> {
    store.list().unwrap().packs
}

/// The opened index of the pack `info`.
pub fn index_of(store: &dyn PackStore, key: &Key, info: &PackInfo) -> Index {
    open_index(key, &info.tag, &store.read_index(&info.id).unwrap()).unwrap()
}

/// A zero byte append by somebody else, `edit` changes the index.
pub fn tamper(store: &dyn PackStore, key: &Key, info: &PackInfo, edit: impl Fn(&mut Index)) {
    let text = store.read_index(&info.id).unwrap();
    let mut idx = open_index(key, &info.tag, &text).unwrap();
    edit(&mut idx);
    let next = seal_index(key, &idx).unwrap();
    store.append(&info.id, info.len, &text, &[], &next).unwrap();
}

/// Plain size whose sealed segment reaches the target but stays a normal session.
pub const FILL: usize = TARGET as usize - 10;
