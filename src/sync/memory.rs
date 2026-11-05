//! In-memory backend for tests and dry runs of the engine.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use super::backend::{BackendError, RemoteEntry, SyncBackend};

#[derive(Default)]
struct Inner {
    entries: BTreeMap<String, (String, Vec<u8>)>,
    next: u64,
    failing: HashSet<String>,
}

/// Remote that lives in a `Mutex`, ids are `mem-<n>`.
#[derive(Default)]
pub struct MemoryBackend {
    inner: Mutex<Inner>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every later push, pull and delete for `id` fails.
    pub fn fail_on(&self, id: &str) {
        self.lock().failing.insert(id.to_string());
    }

    /// Stored blob, for assertions.
    pub fn blob(&self, id: &str) -> Option<Vec<u8>> {
        self.lock().entries.get(id).map(|(_, blob)| blob.clone())
    }

    /// Overwrite a stored blob, for tamper tests.
    pub fn set_blob(&self, id: &str, blob: Vec<u8>) {
        if let Some(entry) = self.lock().entries.get_mut(id) {
            entry.1 = blob;
        }
    }

    /// Number of stored entries.
    pub fn len(&self) -> usize {
        self.lock().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Inner {
    fn guard(&self, id: &str) -> Result<(), BackendError> {
        if self.failing.contains(id) {
            return Err(BackendError::Unreachable(format!(
                "injected failure for {id}"
            )));
        }
        Ok(())
    }

    fn known(&self, id: &str) -> Result<(), BackendError> {
        self.guard(id)?;
        if !self.entries.contains_key(id) {
            return Err(BackendError::Other(format!("unknown remote id {id}")));
        }
        Ok(())
    }
}

impl SyncBackend for MemoryBackend {
    fn name(&self) -> &'static str {
        "memory"
    }

    fn check(&self) -> Result<(), BackendError> {
        Ok(())
    }

    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError> {
        let inner = self.lock();
        Ok(inner
            .entries
            .iter()
            .map(|(id, (token, _))| RemoteEntry {
                id: id.clone(),
                meta_token: token.clone(),
            })
            .collect())
    }

    fn push(&self, id: Option<&str>, token: &str, blob: &[u8]) -> Result<String, BackendError> {
        let mut inner = self.lock();
        let id = match id {
            Some(id) => {
                inner.known(id)?;
                id.to_string()
            }
            None => {
                inner.next += 1;
                format!("mem-{}", inner.next)
            }
        };
        inner
            .entries
            .insert(id.clone(), (token.to_string(), blob.to_vec()));
        Ok(id)
    }

    fn pull(&self, id: &str) -> Result<Vec<u8>, BackendError> {
        let inner = self.lock();
        inner.known(id)?;
        Ok(inner.entries[id].1.clone())
    }

    fn delete(&self, id: &str) -> Result<(), BackendError> {
        let mut inner = self.lock();
        inner.known(id)?;
        inner.entries.remove(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_without_id_creates_new_ids() {
        let b = MemoryBackend::new();
        let a = b.push(None, "t1", b"one").unwrap();
        let c = b.push(None, "t2", b"two").unwrap();
        assert_ne!(a, c);
        assert_eq!(b.len(), 2);
        assert_eq!(b.pull(&a).unwrap(), b"one");
    }

    #[test]
    fn push_with_id_replaces_in_place() {
        let b = MemoryBackend::new();
        let id = b.push(None, "t1", b"one").unwrap();
        let same = b.push(Some(&id), "t2", b"two").unwrap();
        assert_eq!(same, id);
        assert_eq!(b.len(), 1);
        assert_eq!(
            b.list().unwrap(),
            vec![RemoteEntry {
                id,
                meta_token: "t2".into()
            }]
        );
        assert_eq!(b.blob(&same), Some(b"two".to_vec()));
    }

    #[test]
    fn pull_of_unknown_id_errors() {
        let b = MemoryBackend::new();
        assert!(matches!(b.pull("nope"), Err(BackendError::Other(_))));
    }

    #[test]
    fn delete_removes_and_unknown_errors() {
        let b = MemoryBackend::new();
        let id = b.push(None, "t", b"x").unwrap();
        b.delete(&id).unwrap();
        assert!(b.is_empty());
        assert!(b.delete(&id).is_err());
    }

    #[test]
    fn fail_on_injects_errors_for_that_id_only() {
        let b = MemoryBackend::new();
        let bad = b.push(None, "t1", b"x").unwrap();
        let good = b.push(None, "t2", b"y").unwrap();
        b.fail_on(&bad);
        assert!(b.pull(&bad).is_err());
        assert!(b.push(Some(&bad), "t", b"z").is_err());
        assert!(b.delete(&bad).is_err());
        assert_eq!(b.pull(&good).unwrap(), b"y");
    }

    #[test]
    fn set_blob_replaces_bytes() {
        let b = MemoryBackend::new();
        let id = b.push(None, "t", b"x").unwrap();
        b.set_blob(&id, b"tampered".to_vec());
        assert_eq!(b.pull(&id).unwrap(), b"tampered");
    }
}
