//! In-memory pack store for tests.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::sync::Mutex;

use super::{Listing, PackInfo, PackStore, Segment, StoreError};

struct Pack {
    tag: String,
    data: Vec<u8>,
    index: String,
}

#[derive(Default)]
struct Inner {
    packs: BTreeMap<String, Pack>,
    next: u64,
}

/// Packs in a `Mutex`, ids are `mem-<n>`. One lock around check and write models the git ref.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl PackStore for MemoryStore {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn check(&self) -> Result<(), StoreError> {
        Ok(())
    }

    fn list(&self) -> Result<Listing, StoreError> {
        let inner = self.lock();
        let packs = inner
            .packs
            .iter()
            .map(|(id, p)| PackInfo {
                id: id.clone(),
                tag: p.tag.to_uppercase(),
                len: p.data.len() as u64,
                updated_at: String::new(),
            })
            .collect();
        Ok(Listing {
            packs,
            legacy: Vec::new(),
        })
    }

    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        let inner = self.lock();
        inner
            .packs
            .get(id)
            .map(|p| p.index.clone())
            .ok_or(StoreError::NotFound)
    }

    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        let mut inner = self.lock();
        inner.next += 1;
        let id = format!("mem-{}", inner.next);
        let pack = Pack {
            tag: tag.to_string(),
            data: Vec::new(),
            index: index.to_string(),
        };
        inner.packs.insert(id.clone(), pack);
        Ok(id)
    }

    fn append(
        &self,
        id: &str,
        expected_len: u64,
        _expected_index: &str,
        segments: &[Segment],
        index: &str,
    ) -> Result<(), StoreError> {
        let mut inner = self.lock();
        let pack = inner.packs.get_mut(id).ok_or(StoreError::NotFound)?;
        if expected_len < pack.data.len() as u64 {
            return Err(StoreError::Moved(String::new()));
        }
        if segments.is_empty() {
            return Ok(());
        }
        let mut failed = None;
        for s in segments {
            if let Err(e) = (s.write)(&mut pack.data) {
                failed = Some(StoreError::Other(e.to_string()));
            }
        }
        pack.index = index.to_string();
        failed.map_or(Ok(()), Err)
    }

    fn read(&self, _id: &str, _offset: u64, _len: u64) -> Result<Box<dyn Read>, StoreError> {
        Ok(Box::new(Cursor::new(Vec::new())))
    }

    fn delete(&self, _id: &str) -> Result<(), StoreError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    contract_tests!(|| (MemoryStore::new(), ()));
}
