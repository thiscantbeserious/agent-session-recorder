//! Directory backed fake remote, selected only by the binary's test switch
//! `AGR_SYNC_TEST_BACKEND_DIR`. Never used for real data.

use std::fs;
use std::io::Read;
use std::path::PathBuf;

use super::backend::{BackendError, BlobSource, RemoteEntry, SyncBackend};

/// `<id>.blob`, `<id>.manifest` and `<id>.desc` per entry inside `dir`, same rules as the
/// memory backend (no `.desc` means pending and unlisted).
pub struct FileBackend {
    dir: PathBuf,
}

impl FileBackend {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str, ext: &str) -> PathBuf {
        self.dir.join(format!("{id}.{ext}"))
    }

    /// `file-<n>`, the counter file keeps ids unique after deletes.
    fn next_id(&self) -> Result<String, BackendError> {
        let counter = self.dir.join("counter");
        let n: u64 = fs::read_to_string(&counter)
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(0)
            + 1;
        fs::write(&counter, n.to_string()).map_err(io_err)?;
        Ok(format!("file-{n}"))
    }

    fn known(&self, id: &str) -> Result<(), BackendError> {
        if self.path(id, "blob").exists() {
            Ok(())
        } else {
            Err(BackendError::NotFound)
        }
    }

    /// Streams into `<id>.blob.tmp` and renames, a failing source leaves the stored blob alone.
    fn write_blob(&self, id: &str, blob: BlobSource) -> Result<(), BackendError> {
        let tmp = self.path(id, "blob.tmp");
        let written = fs::File::create(&tmp).and_then(|mut f| blob(&mut f));
        let done = written.and_then(|()| fs::rename(&tmp, self.path(id, "blob")));
        if done.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        done.map_err(io_err)
    }

    fn manifest(&self, id: &str) -> Option<String> {
        fs::read_to_string(self.path(id, "manifest")).ok()
    }
}

fn io_err(e: std::io::Error) -> BackendError {
    BackendError::Other(e.to_string())
}

/// Zero padded counter of `file-<n>`, sorts like the creation order.
fn created_at(id: &str) -> String {
    let n: u64 = id.trim_start_matches("file-").parse().unwrap_or(0);
    format!("{n:010}")
}

impl SyncBackend for FileBackend {
    fn name(&self) -> &'static str {
        "file"
    }

    fn check(&self) -> Result<(), BackendError> {
        fs::create_dir_all(&self.dir).map_err(io_err)
    }

    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError> {
        let mut entries = Vec::new();
        for file in fs::read_dir(&self.dir).map_err(io_err)? {
            let name = file
                .map_err(io_err)?
                .file_name()
                .to_string_lossy()
                .into_owned();
            if let Some(id) = name.strip_suffix(".desc") {
                let meta_token = fs::read_to_string(self.path(id, "desc")).map_err(io_err)?;
                let created_at = created_at(id);
                entries.push(RemoteEntry {
                    id: id.to_string(),
                    meta_token,
                    created_at,
                });
            }
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(entries)
    }

    fn create(&self) -> Result<String, BackendError> {
        let id = self.next_id()?;
        fs::write(self.path(&id, "blob"), b"").map_err(io_err)?;
        Ok(id)
    }

    fn push(
        &self,
        id: &str,
        expected: Option<&str>,
        manifest: &str,
        blob: BlobSource,
    ) -> Result<(), BackendError> {
        self.known(id)?;
        let current = self.manifest(id);
        if current.as_deref() != expected {
            return Err(BackendError::Changed(current));
        }
        self.write_blob(id, blob)?;
        fs::write(self.path(id, "manifest"), manifest).map_err(io_err)
    }

    fn describe(&self, id: &str, token: &str) -> Result<(), BackendError> {
        self.known(id)?;
        fs::write(self.path(id, "desc"), token).map_err(io_err)
    }

    fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn Read>), BackendError> {
        self.known(id)?;
        let file = fs::File::open(self.path(id, "blob")).map_err(io_err)?;
        Ok((self.manifest(id), Box::new(file)))
    }

    fn delete(&self, id: &str) -> Result<(), BackendError> {
        self.known(id)?;
        for ext in ["blob", "manifest", "desc"] {
            let _ = fs::remove_file(self.path(id, ext));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::backend::{pull_bytes, push_bytes};
    use super::*;

    fn backend() -> (tempfile::TempDir, FileBackend) {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());
        (dir, backend)
    }

    fn pushed(b: &FileBackend, manifest: &str, blob: &[u8]) -> String {
        let id = b.create().unwrap();
        push_bytes(b, &id, None, manifest, blob).unwrap();
        id
    }

    #[test]
    fn create_is_pending_until_described_and_created_at_grows() {
        let (_dir, b) = backend();
        let a = b.create().unwrap();
        let c = b.create().unwrap();
        assert_ne!(a, c);
        assert!(b.list().unwrap().is_empty());
        b.describe(&a, "ta").unwrap();
        b.describe(&c, "tc").unwrap();
        let list = b.list().unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].created_at < list[1].created_at, "{list:?}");
        assert_eq!(list[1].meta_token, "tc");
    }

    #[test]
    fn push_swaps_only_on_the_current_manifest() {
        let (_dir, b) = backend();
        let id = pushed(&b, "m1", b"one");
        assert_eq!(
            pull_bytes(&b, &id).unwrap(),
            (Some("m1".to_string()), b"one".to_vec())
        );
        push_bytes(&b, &id, Some("m1"), "m2", b"two").unwrap();
        let stale = push_bytes(&b, &id, Some("m1"), "m3", b"three");
        assert_eq!(stale, Err(BackendError::Changed(Some("m2".into()))));
        let none = push_bytes(&b, &id, None, "m3", b"three");
        assert_eq!(none, Err(BackendError::Changed(Some("m2".into()))));
        assert_eq!(
            pull_bytes(&b, &id).unwrap(),
            (Some("m2".to_string()), b"two".to_vec())
        );
    }

    #[test]
    fn pending_entry_pulls_without_manifest_and_stale_expected_is_changed_none() {
        let (_dir, b) = backend();
        let id = b.create().unwrap();
        assert_eq!(pull_bytes(&b, &id).unwrap(), (None, vec![]));
        assert_eq!(
            push_bytes(&b, &id, Some("x"), "m", b"b"),
            Err(BackendError::Changed(None))
        );
    }

    #[test]
    fn unknown_id_is_not_found_and_delete_removes() {
        let (_dir, b) = backend();
        assert_eq!(
            push_bytes(&b, "nope", None, "m", b"x"),
            Err(BackendError::NotFound)
        );
        assert_eq!(b.describe("nope", "t"), Err(BackendError::NotFound));
        assert_eq!(pull_bytes(&b, "nope"), Err(BackendError::NotFound));
        let id = pushed(&b, "m", b"x");
        b.describe(&id, "m").unwrap();
        b.delete(&id).unwrap();
        assert!(b.list().unwrap().is_empty());
        assert_eq!(pull_bytes(&b, &id), Err(BackendError::NotFound));
        assert_eq!(b.delete(&id), Err(BackendError::NotFound));
    }

    #[test]
    fn ids_are_not_reused_after_a_delete() {
        let (_dir, b) = backend();
        let first = b.create().unwrap();
        b.delete(&first).unwrap();
        assert_ne!(b.create().unwrap(), first);
    }

    #[test]
    fn the_source_runs_only_after_the_compare_and_an_error_stores_nothing() {
        let (_dir, b) = backend();
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
