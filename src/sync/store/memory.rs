//! In-memory pack store for tests.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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
    /// Reads of these packs fail after that many bytes.
    cut: HashMap<String, usize>,
    /// Every index text an append wrote, in order.
    indexes: Vec<String>,
    /// Pack ids the pack layer called `sealed` for, in order.
    sealed: Vec<String>,
    /// Successful appends so far.
    appends: usize,
}

/// Packs in a `Mutex`, ids are `mem-<n>`. One lock around check and write models the git ref.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
    slots: Option<usize>,
    lists: AtomicUsize,
    list_fails: AtomicBool,
    list_fails_once: AtomicBool,
    before_append: Mutex<Option<AppendHook>>,
}

/// Runs before every append with the store and the pack id, a writer that wins the race.
type AppendHook = Arc<dyn Fn(&MemoryStore, &str) + Send + Sync>;

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every later `read_index` for `id` fails, for the abort tests of the pack layer.
    pub fn fail_read_index(&self, id: &str) {
        self.lock().failing.insert(id.to_string());
    }

    /// Every later `read` of `id` yields `bytes` bytes, then fails, a place that dies mid-stream.
    pub fn fail_read_after(&self, id: &str, bytes: usize) {
        self.lock().cut.insert(id.to_string(), bytes);
    }

    /// Flips one bit of the last data byte of `id` (an AEAD tag byte), a damaged place for the read tests.
    pub fn corrupt(&self, id: &str) {
        if let Some(p) = self.lock().packs.get_mut(id) {
            p.data.last_mut().iter_mut().for_each(|b| **b ^= 1);
        }
    }

    /// Every index text the appends of this store wrote, in order.
    pub fn index_texts(&self) -> Vec<String> {
        self.lock().indexes.clone()
    }

    /// How many appends went through.
    pub fn appends(&self) -> usize {
        self.lock().appends
    }

    /// The ids `sealed` was called for, in order.
    pub fn sealed_ids(&self) -> Vec<String> {
        self.lock().sealed.clone()
    }

    /// How often `list` ran.
    pub fn list_calls(&self) -> usize {
        self.lists.load(Ordering::SeqCst)
    }

    /// Every later `list` fails.
    pub fn fail_list(&self) {
        self.list_fails.store(true, Ordering::SeqCst);
    }

    /// The next `list` fails, the ones after it work again.
    pub fn fail_next_list(&self) {
        self.list_fails_once.store(true, Ordering::SeqCst);
    }

    /// `hook` runs before every later append, outside the store lock.
    pub fn before_append(&self, hook: impl Fn(&MemoryStore, &str) + Send + Sync + 'static) {
        *self
            .before_append
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(hook));
    }

    /// Store whose packs hold at most `n` file slots, one per segment, for `fits`.
    pub fn with_slots(n: usize) -> Self {
        Self {
            slots: Some(n),
            ..Self::default()
        }
    }

    /// The hook is out of its slot while it runs, its own appends do not call it again.
    fn run_hook(&self, id: &str) {
        let slot = || {
            (self.before_append)
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        let hook = slot().take();
        if let Some(hook) = hook {
            hook(self, id);
            *slot() = Some(hook);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The rest of a read that was cut off.
struct Dies;

impl Read for Dies {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "read cut off on purpose",
        ))
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
        self.lists.fetch_add(1, Ordering::SeqCst);
        let once = self.list_fails_once.swap(false, Ordering::SeqCst);
        if once || self.list_fails.load(Ordering::SeqCst) {
            return Err(StoreError::Unreachable("list failed on purpose".into()));
        }
        let inner = self.lock();
        let packs = inner
            .packs
            .iter()
            .map(|(id, p)| PackInfo {
                id: id.clone(),
                tag: p.tag.clone(),
                len: p.data.len() as u64,
                updated_at: String::new(),
                files: p.files,
            })
            .collect();
        Ok(Listing {
            packs,
            legacy: Vec::new(),
        })
    }

    fn fits(&self, info: &PackInfo, lens: &[u64]) -> bool {
        let Some(max) = self.slots else { return true };
        info.files + lens.len() <= max
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
        self.run_hook(id);
        let mut inner = self.lock();
        let pack = inner.packs.get_mut(id).ok_or(StoreError::NotFound)?;
        if pack.data.len() as u64 != expected_len || pack.index != expected_index {
            return Err(StoreError::Moved(pack.index.clone()));
        }
        let mut grown = pack.data.clone();
        for s in segments {
            let before = grown.len() as u64;
            (s.write)(&mut grown).map_err(|e| StoreError::Other(e.to_string()))?;
            if grown.len() as u64 - before != s.len {
                return Err(StoreError::Other(
                    "segment len differs from its bytes".into(),
                ));
            }
        }
        pack.data = grown;
        pack.files += segments.len();
        pack.index = index.to_string();
        inner.indexes.push(index.to_string());
        inner.appends += 1;
        Ok(())
    }

    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        let inner = self.lock();
        let data = &inner.packs.get(id).ok_or(StoreError::NotFound)?.data;
        let end = offset.checked_add(len).ok_or(StoreError::NotFound)?;
        let range = data
            .get(offset as usize..end as usize)
            .ok_or(StoreError::NotFound)?;
        let Some(&cut) = inner.cut.get(id) else {
            return Ok(Box::new(Cursor::new(range.to_vec())));
        };
        let head = Cursor::new(range[..cut.min(range.len())].to_vec());
        Ok(Box::new(head.chain(Dies)))
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
    use super::{MemoryStore, PackStore, Segment};
    use std::io::Write;

    contract_tests!(|| (MemoryStore::new(), ()));

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
}
