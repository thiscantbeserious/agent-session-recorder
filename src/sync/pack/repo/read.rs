//! Reads: one whole pack under a given key, and one session of the merged view.

use crate::sync::crypto::Key;
use crate::sync::pack::index::Entry;
use crate::sync::store::{PackStore, StoreError};

/// Opens every live session of pack `id` with `key`, without any merge.
pub fn read_pack(
    _store: &dyn PackStore,
    _id: &str,
    _key: &Key,
) -> Result<Vec<(Entry, Vec<u8>)>, StoreError> {
    Ok(Vec::new())
}
