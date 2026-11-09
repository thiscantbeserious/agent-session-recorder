//! Reads: one whole pack under a given key, and one session of the merged view.

use crate::sync::crypto::Key;
use crate::sync::pack::index::{open_index, Entry};
use crate::sync::pack::stream::{open_range, RangeError};
use crate::sync::store::{PackStore, StoreError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReadError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Range(#[from] RangeError),
    #[error("pack index does not open with this key")]
    Index,
    #[error("session does not match its recorded hash")]
    HashMismatch,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Opens every live session of pack `id` with `key`, without any merge. Whoever holds `key`
/// reads this pack and nothing else, the sync key is not involved.
pub fn read_pack(
    store: &dyn PackStore,
    id: &str,
    key: &Key,
) -> Result<Vec<(Entry, Vec<u8>)>, ReadError> {
    let tag = store
        .list()?
        .packs
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(StoreError::NotFound)?
        .tag;
    let index = open_index(key, &tag, &store.read_index(id)?).map_err(|_| ReadError::Index)?;
    let mut out = Vec::new();
    for e in index.entries.into_iter().filter(|e| !e.deleted) {
        let mut plain = Vec::new();
        let (hash, size) = open_range(key, store, id, &tag, &e, &mut plain)?;
        if hash != e.sha256 || size != e.size {
            return Err(ReadError::HashMismatch);
        }
        out.push((e, plain));
    }
    Ok(out)
}
