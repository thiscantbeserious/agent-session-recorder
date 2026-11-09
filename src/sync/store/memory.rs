//! In-memory pack store for tests.

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::Mutex;

use super::{Listing, PackInfo, PackStore, Segment, StoreError};

struct Pack {
    tag: String,
    data: Vec<u8>,
    index: String,
    /// Data files so far, one per segment.
    files: usize,
}

#[derive(Default)]
struct Inner {
    packs: BTreeMap<String, Pack>,
    next: u64,
    failing: HashSet<String>,
}

/// Packs in a `Mutex`, ids are `mem-<n>`. One lock around check and write models the git ref.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
    slots: Option<usize>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every later `read_index` for `id` fails, for the abort tests of the pack layer.
    pub fn fail_read_index(&self, id: &str) {
        self.lock().failing.insert(id.to_string());
    }

    /// Store whose packs hold at most `n` file slots, one per append, for `fits`.
    pub fn with_slots(n: usize) -> Self {
        Self {
            slots: Some(n),
            ..Self::default()
        }
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
                tag: p.tag.clone(),
                len: p.data.len() as u64,
                updated_at: String::new(),
            })
            .collect();
        Ok(Listing {
            packs,
            legacy: Vec::new(),
        })
    }

    fn fits(&self, info: &PackInfo, lens: &[u64]) -> bool {
        let Some(max) = self.slots else { return true };
        let used = self.lock().packs.get(&info.id).map_or(0, |p| p.files);
        used + lens.len() <= max
    }

    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        let inner = self.lock();
        if inner.failing.contains(id) {
            return Err(StoreError::Unreachable(
                "read_index failed on purpose".into(),
            ));
        }
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
            files: 0,
        };
        inner.packs.insert(id.clone(), pack);
        Ok(id)
    }

    fn append(
        &self,
        id: &str,
        expected_len: u64,
        expected_index: &str,
        segments: &[Segment],
        index: &str,
    ) -> Result<(), StoreError> {
        let mut inner = self.lock();
        let pack = inner.packs.get_mut(id).ok_or(StoreError::NotFound)?;
        if pack.data.len() as u64 != expected_len || pack.index != expected_index {
            return Err(StoreError::Moved(pack.index.clone()));
        }
        let mut grown = pack.data.clone();
        for s in segments {
            (s.write)(&mut grown).map_err(|e| StoreError::Other(e.to_string()))?;
        }
        pack.data = grown;
        pack.files += segments.len();
        pack.index = index.to_string();
        Ok(())
    }

    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        let inner = self.lock();
        let data = &inner.packs.get(id).ok_or(StoreError::NotFound)?.data;
        let (from, to) = (offset as usize, (offset + len) as usize);
        let range = data.get(from..to).ok_or(StoreError::NotFound)?;
        Ok(Box::new(Cursor::new(range.to_vec())))
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        self.lock()
            .packs
            .remove(id)
            .map(drop)
            .ok_or(StoreError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    contract_tests!(|| (MemoryStore::new(), ()));

    #[test]
    fn memory_fits_false_when_slots_run_out() {
        let store = MemoryStore::with_slots(2);
        let id = store.create("t", "i0").unwrap();
        let w = |w: &mut dyn std::io::Write| w.write_all(b"a");
        let seg = || Segment { len: 1, write: &w };
        store.append(&id, 0, "i0", &[seg()], "i1").unwrap();
        store.append(&id, 1, "i1", &[seg()], "i2").unwrap();
        let info = store.list().unwrap().packs.remove(0);
        assert!(!store.fits(&info, &[1]));
    }
}
