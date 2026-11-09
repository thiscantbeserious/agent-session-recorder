//! Reads: one whole pack under a given key, and one session of the merged view.

use super::Repo;
use crate::files::codec::atomic_write_raw_with;
use crate::sync::crypto::Key;
use crate::sync::pack::index::{open_index, Entry};
use crate::sync::pack::merge::Place;
use crate::sync::pack::stream::{open_range, RangeError};
use crate::sync::store::{PackStore, StoreError};
use std::io::Write;
use std::path::Path;
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
    #[error("no such session: {0}")]
    Unknown(String),
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

impl ReadError {
    /// The place is gone, the next one may still hold the session.
    fn is_not_found(&self) -> bool {
        let gone = |e: &StoreError| matches!(e, StoreError::NotFound);
        match self {
            Self::Store(e) | Self::Range(RangeError::Store(e)) => gone(e),
            _ => false,
        }
    }
}

impl Repo<'_> {
    /// Opens the session `path` of the merged view into `target`: a hidden temp file in its
    /// directory, renamed only after hash and size match the index, removed on any error.
    pub fn read_session(&self, path: &str, target: &Path) -> Result<(), ReadError> {
        let merged = match self.view().get(path) {
            Some(m) if !m.is_deleted() => m,
            _ => return Err(ReadError::Unknown(path.into())),
        };
        let places: Vec<_> = merged.places.iter().collect();
        atomic_write_raw_with(target, |out| {
            let mut last = ReadError::Unknown(path.into());
            for place in &places {
                let entry = self.entry_of(path, merged.gen, place);
                match entry.map(|e| self.open_into(&place.pack, &e, out)) {
                    Some(Err(e)) if e.is_not_found() => {}
                    None => {}
                    Some(Err(e)) => return Err(e.into()),
                    Some(Ok(())) => return Ok(()),
                }
                last = ReadError::Store(StoreError::NotFound);
            }
            Err(last.into())
        })
        .map_err(|e| match e.downcast::<ReadError>() {
            Ok(read) => read,
            Err(other) => ReadError::Io(std::io::Error::other(other.to_string())),
        })
    }

    /// The index entry behind a place, it carries the plaintext size the view does not.
    fn entry_of(&self, path: &str, gen: u64, place: &Place) -> Option<Entry> {
        let (_, index) = self.packs().get(&place.pack)?;
        let at = |e: &&Entry| e.path == path && e.gen == gen && e.offset == place.offset;
        index.entries.iter().find(at).cloned()
    }

    fn open_into(&self, id: &str, entry: &Entry, out: &mut dyn Write) -> Result<(), ReadError> {
        let tag = self
            .view()
            .pack(id)
            .map(|m| m.tag.clone())
            .unwrap_or_default();
        let (hash, size) = open_range(self.key, self.store, id, &tag, entry, out)?;
        if hash != entry.sha256 || size != entry.size {
            return Err(ReadError::HashMismatch);
        }
        Ok(())
    }
}
