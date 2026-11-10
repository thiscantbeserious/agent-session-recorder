//! Reads: one whole pack under a given key, and one session of the merged view.

use super::Repo;
use crate::files::codec::atomic_write_raw_with;
use crate::sync::crypto::blob::BlobError;
use crate::sync::crypto::Key;
use crate::sync::pack::index::{open_index, Entry};
use crate::sync::pack::merge::Place;
use crate::sync::pack::stream::{digest_is, open_range, RangeError};
use crate::sync::store::{PackStore, StoreError};
use std::io::{ErrorKind, Write};
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
        let got = open_range(key, store, id, &tag, &e, &mut plain)?;
        if !digest_is(&got, &e.sha256, e.size) {
            return Err(ReadError::HashMismatch);
        }
        out.push((e, plain));
    }
    Ok(out)
}

impl ReadError {
    /// The failure is about this place alone, the next place may still hold the session: not
    /// found, a range the place cannot serve, a failed AEAD tag, a hash mismatch. What applies
    /// to every place stays fatal: a missing tool or login, rate limit, no network, local IO.
    fn is_place_local(&self) -> bool {
        let local = |e: &StoreError| {
            matches!(
                e,
                StoreError::NotFound | StoreError::Moved(_) | StoreError::Other(_)
            )
        };
        match self {
            Self::Store(e) | Self::Range(RangeError::Store(e)) => local(e),
            Self::Range(RangeError::Blob(BlobError::Crypto(_))) | Self::HashMismatch => true,
            Self::Range(RangeError::Blob(BlobError::Io(e))) => e.kind() == ErrorKind::NotFound,
            _ => false,
        }
    }
}

/// A mismatch only says the place is damaged, an earlier error says why the read failed, so
/// the first error that is not a mismatch stays and a mismatch fills an empty slot only.
fn keep_first(slot: &mut Option<ReadError>, e: ReadError) {
    let held = slot
        .as_ref()
        .is_some_and(|h| !matches!(h, ReadError::HashMismatch));
    if !held {
        *slot = Some(e);
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
        let mut last: Option<ReadError> = None;
        for place in &merged.places {
            let Some(entry) = self.entry_of(path, merged.gen, place) else {
                continue;
            };
            match self.read_place(&place.pack, &entry, target) {
                Ok(()) => return Ok(()),
                Err(e) if e.is_place_local() => keep_first(&mut last, e),
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap_or_else(|| ReadError::Unknown(path.into())))
    }

    /// One place into its own temp file, a failed place leaves nothing for the next one.
    fn read_place(&self, pack: &str, entry: &Entry, target: &Path) -> Result<(), ReadError> {
        atomic_write_raw_with(target, |out| Ok(self.open_into(pack, entry, out)?)).map_err(|e| {
            match e.downcast::<ReadError>() {
                Ok(read) => read,
                Err(other) => ReadError::Io(std::io::Error::other(other.to_string())),
            }
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
        let got = open_range(self.key, self.store, id, &tag, entry, out)?;
        if !digest_is(&got, &entry.sha256, entry.size) {
            return Err(ReadError::HashMismatch);
        }
        Ok(())
    }
}
