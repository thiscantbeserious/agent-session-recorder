//! Directory backed fake remote, selected only by the binary's test switch
//! `AGR_SYNC_TEST_BACKEND_DIR`. Never used for real data.

use std::fs;
use std::path::PathBuf;

use super::backend::{BackendError, RemoteEntry, SyncBackend};

/// One `<id>.meta` (token) and one `<id>.blob` per entry inside `dir`.
pub struct FileBackend {
    dir: PathBuf,
}

impl FileBackend {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn meta(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.meta"))
    }

    fn blob(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.blob"))
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
}

fn io_err(e: std::io::Error) -> BackendError {
    BackendError::Other(e.to_string())
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
            if let Some(id) = name.strip_suffix(".meta") {
                let meta_token = fs::read_to_string(self.meta(id)).map_err(io_err)?;
                entries.push(RemoteEntry {
                    id: id.to_string(),
                    meta_token,
                });
            }
        }
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(entries)
    }

    fn push(
        &self,
        id: Option<&str>,
        meta_token: &str,
        blob: &[u8],
    ) -> Result<String, BackendError> {
        let id = match id {
            Some(id) => id.to_string(),
            None => self.next_id()?,
        };
        fs::write(self.blob(&id), blob).map_err(io_err)?;
        fs::write(self.meta(&id), meta_token).map_err(io_err)?;
        Ok(id)
    }

    fn pull(&self, id: &str) -> Result<Vec<u8>, BackendError> {
        fs::read(self.blob(id)).map_err(|_| BackendError::Other(format!("unknown remote id {id}")))
    }

    fn delete(&self, id: &str) -> Result<(), BackendError> {
        fs::remove_file(self.meta(id)).map_err(io_err)?;
        fs::remove_file(self.blob(id)).map_err(io_err)
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

    #[test]
    fn push_creates_ids_lists_and_pulls_back() {
        let (_dir, b) = backend();
        let a = b.push(None, "tok-a", b"blob-a").unwrap();
        let c = b.push(None, "tok-c", b"blob-c").unwrap();
        assert_ne!(a, c);
        let mut list = b.list().unwrap();
        list.sort_by(|x, y| x.id.cmp(&y.id));
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|e| e.id == a && e.meta_token == "tok-a"));
        assert_eq!(b.pull(&c).unwrap(), b"blob-c");
    }

    #[test]
    fn push_with_id_replaces_and_delete_removes() {
        let (_dir, b) = backend();
        let id = b.push(None, "t1", b"one").unwrap();
        assert_eq!(b.push(Some(&id), "t2", b"two").unwrap(), id);
        assert_eq!(b.list().unwrap().len(), 1);
        assert_eq!(b.pull(&id).unwrap(), b"two");
        b.delete(&id).unwrap();
        assert!(b.list().unwrap().is_empty());
        assert!(b.pull(&id).is_err());
    }

    #[test]
    fn ids_are_not_reused_after_a_delete() {
        let (_dir, b) = backend();
        let first = b.push(None, "t", b"x").unwrap();
        b.delete(&first).unwrap();
        assert_ne!(b.push(None, "t", b"x").unwrap(), first);
    }
}
