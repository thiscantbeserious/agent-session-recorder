//! Directory backed fake remote, selected only by the binary's test switch
//! `AGR_SYNC_TEST_BACKEND_DIR`. Never used for real data.

use std::fs;
use std::path::PathBuf;

use super::backend::{BackendError, RemoteEntry, SyncBackend};

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
        blob: &[u8],
    ) -> Result<(), BackendError> {
        self.known(id)?;
        let current = self.manifest(id);
        if current.as_deref() != expected {
            return Err(BackendError::Changed(current));
        }
        fs::write(self.path(id, "blob"), blob).map_err(io_err)?;
        fs::write(self.path(id, "manifest"), manifest).map_err(io_err)
    }

    fn describe(&self, id: &str, token: &str) -> Result<(), BackendError> {
        self.known(id)?;
        fs::write(self.path(id, "desc"), token).map_err(io_err)
    }

    fn pull(&self, id: &str) -> Result<(Option<String>, Vec<u8>), BackendError> {
        self.known(id)?;
        let blob = fs::read(self.path(id, "blob")).map_err(io_err)?;
        Ok((self.manifest(id), blob))
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
    use super::*;

    fn backend() -> (tempfile::TempDir, FileBackend) {
        let dir = tempfile::tempdir().unwrap();
        let backend = FileBackend::new(dir.path().to_path_buf());
        (dir, backend)
    }

    fn pushed(b: &FileBackend, manifest: &str, blob: &[u8]) -> String {
        let id = b.create().unwrap();
        b.push(&id, None, manifest, blob).unwrap();
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
            b.pull(&id).unwrap(),
            (Some("m1".to_string()), b"one".to_vec())
        );
        b.push(&id, Some("m1"), "m2", b"two").unwrap();
        let stale = b.push(&id, Some("m1"), "m3", b"three");
        assert_eq!(stale, Err(BackendError::Changed(Some("m2".into()))));
        let none = b.push(&id, None, "m3", b"three");
        assert_eq!(none, Err(BackendError::Changed(Some("m2".into()))));
        assert_eq!(
            b.pull(&id).unwrap(),
            (Some("m2".to_string()), b"two".to_vec())
        );
    }

    #[test]
    fn pending_entry_pulls_without_manifest_and_stale_expected_is_changed_none() {
        let (_dir, b) = backend();
        let id = b.create().unwrap();
        assert_eq!(b.pull(&id).unwrap(), (None, vec![]));
        assert_eq!(
            b.push(&id, Some("x"), "m", b"b"),
            Err(BackendError::Changed(None))
        );
    }

    #[test]
    fn unknown_id_is_not_found_and_delete_removes() {
        let (_dir, b) = backend();
        assert_eq!(b.push("nope", None, "m", b"x"), Err(BackendError::NotFound));
        assert_eq!(b.describe("nope", "t"), Err(BackendError::NotFound));
        assert_eq!(b.pull("nope"), Err(BackendError::NotFound));
        let id = pushed(&b, "m", b"x");
        b.describe(&id, "m").unwrap();
        b.delete(&id).unwrap();
        assert!(b.list().unwrap().is_empty());
        assert_eq!(b.pull(&id), Err(BackendError::NotFound));
        assert_eq!(b.delete(&id), Err(BackendError::NotFound));
    }

    #[test]
    fn ids_are_not_reused_after_a_delete() {
        let (_dir, b) = backend();
        let first = b.create().unwrap();
        b.delete(&first).unwrap();
        assert_ne!(b.create().unwrap(), first);
    }
}
