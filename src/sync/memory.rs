//! In-memory backend for tests and dry runs of the engine.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use std::io::{Cursor, Read};

use super::backend::{BackendError, BlobSource, RemoteEntry, SyncBackend};

#[derive(Default, Clone)]
struct Stored {
    created: u64,
    description: Option<String>,
    manifest: Option<String>,
    blob: Vec<u8>,
}

#[derive(Default)]
struct Inner {
    entries: BTreeMap<String, Stored>,
    next: u64,
    failing: HashSet<String>,
}

/// Remote that lives in a `Mutex`, ids are `mem-<n>`. One lock around the whole compare and
/// write models the ref check of git.
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

    /// Every later push, describe, pull and delete for `id` fails.
    pub fn fail_on(&self, id: &str) {
        self.lock().failing.insert(id.to_string());
    }

    /// Stored blob, for assertions.
    pub fn blob(&self, id: &str) -> Option<Vec<u8>> {
        self.lock().entries.get(id).map(|e| e.blob.clone())
    }

    /// Stored manifest, for assertions.
    pub fn manifest(&self, id: &str) -> Option<String> {
        self.lock().entries.get(id)?.manifest.clone()
    }

    /// Stored description, for assertions.
    pub fn description(&self, id: &str) -> Option<String> {
        self.lock().entries.get(id)?.description.clone()
    }

    /// Overwrite a stored blob, for tamper tests.
    pub fn set_blob(&self, id: &str, blob: Vec<u8>) {
        self.edit(id, |e| e.blob = blob);
    }

    /// Overwrite the stored manifest, for tamper and rollback tests.
    pub fn set_manifest(&self, id: &str, manifest: Option<String>) {
        self.edit(id, |e| e.manifest = manifest);
    }

    /// Overwrite the description without a gen check, models a stalled PATCH.
    pub fn set_description(&self, id: &str, description: Option<String>) {
        self.edit(id, |e| e.description = description);
    }

    /// Test hook, one whole remote state in one go (new entry when `id` is `None`), manifest
    /// and description both get `token`, no compare. Models another machine's finished push.
    pub fn put(&self, id: Option<&str>, token: &str, blob: &[u8]) -> String {
        let id = match id {
            Some(id) => id.to_string(),
            None => self.create().expect("memory create"),
        };
        self.set_manifest(&id, Some(token.to_string()));
        self.set_description(&id, Some(token.to_string()));
        self.set_blob(&id, blob.to_vec());
        id
    }

    fn edit(&self, id: &str, change: impl FnOnce(&mut Stored)) {
        if let Some(entry) = self.lock().entries.get_mut(id) {
            change(entry);
        }
    }

    /// Number of stored entries, pending ones too.
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

    fn known(&mut self, id: &str) -> Result<&mut Stored, BackendError> {
        self.guard(id)?;
        self.entries.get_mut(id).ok_or(BackendError::NotFound)
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
            .filter_map(|(id, e)| {
                Some(RemoteEntry {
                    id: id.clone(),
                    meta_token: e.description.clone()?,
                    created_at: format!("{:010}", e.created),
                })
            })
            .collect())
    }

    fn create(&self) -> Result<String, BackendError> {
        let mut inner = self.lock();
        inner.next += 1;
        let id = format!("mem-{}", inner.next);
        let created = inner.next;
        inner.entries.insert(
            id.clone(),
            Stored {
                created,
                ..Stored::default()
            },
        );
        Ok(id)
    }

    fn push(
        &self,
        id: &str,
        expected: Option<&str>,
        manifest: &str,
        blob: BlobSource,
    ) -> Result<(), BackendError> {
        let mut inner = self.lock();
        let entry = inner.known(id)?;
        if entry.manifest.as_deref() != expected {
            return Err(BackendError::Changed(entry.manifest.clone()));
        }
        let mut bytes = Vec::new();
        blob(&mut bytes).map_err(|e| BackendError::Other(e.to_string()))?;
        entry.manifest = Some(manifest.to_string());
        entry.blob = bytes;
        Ok(())
    }

    fn describe(&self, id: &str, token: &str) -> Result<(), BackendError> {
        self.lock().known(id)?.description = Some(token.to_string());
        Ok(())
    }

    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), BackendError> {
        let mut inner = self.lock();
        let entry = inner.known(id)?;
        Ok((
            entry.manifest.clone(),
            Box::new(Cursor::new(entry.blob.clone())),
        ))
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
    use super::super::backend::{pull_bytes, push_bytes};
    use super::*;

    fn pushed(b: &MemoryBackend, manifest: &str, blob: &[u8]) -> String {
        let id = b.create().unwrap();
        push_bytes(b, &id, None, manifest, blob).unwrap();
        id
    }

    #[test]
    fn create_is_pending_unlisted_and_created_at_grows() {
        let b = MemoryBackend::new();
        let a = b.create().unwrap();
        let c = b.create().unwrap();
        assert_ne!(a, c);
        assert!(b.list().unwrap().is_empty());
        assert_eq!(b.len(), 2);
        b.describe(&a, "ta").unwrap();
        b.describe(&c, "tc").unwrap();
        let list = b.list().unwrap();
        assert!(list[0].created_at < list[1].created_at, "{list:?}");
    }

    #[test]
    fn push_with_the_current_token_stores_and_leaves_the_description() {
        let b = MemoryBackend::new();
        let id = pushed(&b, "m1", b"one");
        assert_eq!(
            (b.manifest(&id).as_deref(), b.blob(&id)),
            (Some("m1"), Some(b"one".to_vec()))
        );
        b.describe(&id, "m1").unwrap();
        push_bytes(&b, &id, Some("m1"), "m2", b"two").unwrap();
        assert_eq!(
            (b.manifest(&id).as_deref(), b.blob(&id)),
            (Some("m2"), Some(b"two".to_vec()))
        );
        assert_eq!(b.description(&id).as_deref(), Some("m1"));
        b.describe(&id, "m2").unwrap();
        assert_eq!(b.description(&id).as_deref(), Some("m2"));
    }

    #[test]
    fn push_with_a_stale_expected_returns_current_and_stores_nothing() {
        let b = MemoryBackend::new();
        let id = pushed(&b, "m1", b"one");
        let stale = push_bytes(&b, &id, Some("old"), "m2", b"two");
        assert_eq!(stale, Err(BackendError::Changed(Some("m1".into()))));
        let none = push_bytes(&b, &id, None, "m3", b"three");
        assert_eq!(none, Err(BackendError::Changed(Some("m1".into()))));
        assert_eq!(
            (b.manifest(&id).as_deref(), b.blob(&id)),
            (Some("m1"), Some(b"one".to_vec()))
        );
    }

    #[test]
    fn stale_expected_on_a_pending_entry_returns_changed_none() {
        let b = MemoryBackend::new();
        let id = b.create().unwrap();
        assert_eq!(
            push_bytes(&b, &id, Some("x"), "m", b"b"),
            Err(BackendError::Changed(None))
        );
        assert_eq!(b.blob(&id), Some(vec![]));
    }

    #[test]
    fn unknown_id_is_not_found_everywhere() {
        let b = MemoryBackend::new();
        assert_eq!(
            push_bytes(&b, "nope", None, "m", b"x"),
            Err(BackendError::NotFound)
        );
        assert_eq!(b.describe("nope", "t"), Err(BackendError::NotFound));
        assert_eq!(pull_bytes(&b, "nope"), Err(BackendError::NotFound));
        assert_eq!(b.delete("nope"), Err(BackendError::NotFound));
    }

    #[test]
    fn pull_returns_manifest_and_blob_and_none_for_pending() {
        let b = MemoryBackend::new();
        let id = pushed(&b, "m1", b"one");
        assert_eq!(
            pull_bytes(&b, &id).unwrap(),
            (Some("m1".to_string()), b"one".to_vec())
        );
        let pending = b.create().unwrap();
        assert_eq!(pull_bytes(&b, &pending).unwrap(), (None, vec![]));
    }

    #[test]
    fn hooks_tamper_roll_back_and_stall() {
        let b = MemoryBackend::new();
        let id = pushed(&b, "m1", b"x");
        b.describe(&id, "m1").unwrap();
        b.set_blob(&id, b"tampered".to_vec());
        b.set_manifest(&id, Some("m0".into()));
        b.set_description(&id, Some("old".into()));
        assert_eq!(
            pull_bytes(&b, &id).unwrap(),
            (Some("m0".to_string()), b"tampered".to_vec())
        );
        assert_eq!(b.list().unwrap()[0].meta_token, "old");
        b.set_manifest(&id, None);
        assert_eq!(pull_bytes(&b, &id).unwrap().0, None);
    }

    #[test]
    fn put_sets_the_whole_state_without_a_compare() {
        let b = MemoryBackend::new();
        let id = b.put(None, "t1", b"one");
        assert_eq!(b.put(Some(&id), "t2", b"two"), id);
        assert_eq!(
            pull_bytes(&b, &id).unwrap(),
            (Some("t2".to_string()), b"two".to_vec())
        );
        assert_eq!(b.list().unwrap()[0].meta_token, "t2");
    }

    #[test]
    fn delete_removes_and_unknown_errors() {
        let b = MemoryBackend::new();
        let id = pushed(&b, "m", b"x");
        b.delete(&id).unwrap();
        assert!(b.is_empty());
        assert!(b.delete(&id).is_err());
    }

    #[test]
    fn fail_on_injects_errors_for_that_id_only() {
        let b = MemoryBackend::new();
        let bad = pushed(&b, "m1", b"x");
        let good = pushed(&b, "m2", b"y");
        b.fail_on(&bad);
        assert!(pull_bytes(&b, &bad).is_err());
        assert!(push_bytes(&b, &bad, Some("m1"), "m", b"z").is_err());
        assert!(b.describe(&bad, "t").is_err());
        assert!(b.delete(&bad).is_err());
        assert_eq!(pull_bytes(&b, &good).unwrap().1, b"y");
    }

    #[test]
    fn the_source_runs_only_after_the_compare_and_an_error_stores_nothing() {
        let b = MemoryBackend::new();
        let id = pushed(&b, "m1", b"one");
        let calls = std::cell::Cell::new(0);
        let counted = |w: &mut dyn std::io::Write| {
            calls.set(calls.get() + 1);
            w.write_all(b"two")
        };
        let stale = b.push(&id, Some("old"), "m2", &counted);
        assert_eq!(stale, Err(BackendError::Changed(Some("m1".into()))));
        assert_eq!(calls.get(), 0, "source must not run on a stale expected");
        b.push(&id, Some("m1"), "m2", &counted).unwrap();
        assert_eq!(calls.get(), 1);
        let broken = |w: &mut dyn std::io::Write| {
            w.write_all(b"partial")?;
            Err(std::io::Error::other("disk gone"))
        };
        assert!(b.push(&id, Some("m2"), "m3", &broken).is_err());
        assert_eq!(
            pull_bytes(&b, &id).unwrap(),
            (Some("m2".to_string()), b"two".to_vec())
        );
    }
}
