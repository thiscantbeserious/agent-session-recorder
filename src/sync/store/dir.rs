//! Directory pack store: `<root>/<id>/part.NNNNNN` files plus one `manifest`.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use super::{Listing, PackInfo, PackStore, Segment, StoreError};

#[derive(Serialize, Deserialize)]
struct Manifest {
    tag: String,
    len: u64,
    parts: Vec<String>,
    index: String,
}

/// One directory per pack under `root`.
pub struct DirStore {
    root: PathBuf,
    #[cfg(test)]
    crash_before_manifest: AtomicBool,
}

fn io_err(e: io::Error) -> StoreError {
    match e.kind() {
        io::ErrorKind::NotFound => StoreError::NotFound,
        _ => StoreError::Other(e.to_string()),
    }
}

/// Writes one segment and checks it wrote exactly `len` bytes.
fn write_segment(file: &mut File, s: &Segment) -> Result<(), StoreError> {
    let before = file.stream_position().map_err(io_err)?;
    (s.write)(file).map_err(|e| StoreError::Other(e.to_string()))?;
    let after = file.stream_position().map_err(io_err)?;
    if after - before != s.len {
        return Err(StoreError::Other(
            "segment len differs from its bytes".into(),
        ));
    }
    Ok(())
}

impl DirStore {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            #[cfg(test)]
            crash_before_manifest: AtomicBool::new(false),
        }
    }

    /// The next append writes its part file and then fails before the manifest is renamed.
    #[cfg(test)]
    fn crash_before_manifest(&self) {
        self.crash_before_manifest.store(true, Ordering::SeqCst);
    }

    fn manifest(&self, id: &str) -> Result<Manifest, StoreError> {
        let text = fs::read_to_string(self.root.join(id).join("manifest")).map_err(io_err)?;
        serde_json::from_str(&text).map_err(|e| StoreError::Other(e.to_string()))
    }

    fn write_manifest(&self, id: &str, m: &Manifest) -> Result<(), StoreError> {
        let dir = self.root.join(id);
        let tmp = dir.join("manifest.tmp");
        let text = serde_json::to_string(m).map_err(|e| StoreError::Other(e.to_string()))?;
        fs::write(&tmp, text).map_err(io_err)?;
        fs::rename(tmp, dir.join("manifest")).map_err(io_err)
    }

    /// Deletes part files the manifest does not list, left behind by a crash before the rename.
    fn drop_unlisted(&self, id: &str, m: &Manifest) -> Result<(), StoreError> {
        for e in fs::read_dir(self.root.join(id)).map_err(io_err)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("part.") && !m.parts.contains(&name) {
                fs::remove_file(e.path()).map_err(io_err)?;
            }
        }
        Ok(())
    }

    /// Writes all segments into one new part file, removes it again when a writer fails.
    fn write_part(&self, id: &str, name: &str, segments: &[Segment]) -> Result<u64, StoreError> {
        let path = self.root.join(id).join(name);
        let mut file = File::create(&path).map_err(io_err)?;
        let written = segments
            .iter()
            .try_for_each(|s| write_segment(&mut file, s));
        if let Err(e) = written {
            drop(fs::remove_file(&path));
            return Err(e);
        }
        Ok(file.metadata().map_err(io_err)?.len())
    }
}

impl PackStore for DirStore {
    fn name(&self) -> &'static str {
        "directory"
    }

    fn check(&self) -> Result<(), StoreError> {
        fs::create_dir_all(&self.root).map_err(io_err)
    }

    fn list(&self) -> Result<Listing, StoreError> {
        let mut packs = Vec::new();
        for e in fs::read_dir(&self.root).map_err(io_err)?.flatten() {
            let id = e.file_name().to_string_lossy().into_owned();
            let m = match self.manifest(&id) {
                Ok(m) => m,
                // not a pack directory, nothing to list
                Err(StoreError::NotFound) => continue,
                Err(e) => return Err(e),
            };
            packs.push(PackInfo {
                id,
                tag: m.tag,
                len: m.len,
                updated_at: String::new(),
            });
        }
        packs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(Listing {
            packs,
            legacy: Vec::new(),
        })
    }

    fn read_index(&self, id: &str) -> Result<String, StoreError> {
        Ok(self.manifest(id)?.index)
    }

    fn create(&self, tag: &str, index: &str) -> Result<String, StoreError> {
        let id = (1..)
            .map(|n| format!("pack-{n}"))
            .find(|id| !self.root.join(id).exists())
            .unwrap_or_default();
        fs::create_dir_all(self.root.join(&id)).map_err(io_err)?;
        let m = Manifest {
            tag: tag.into(),
            len: 0,
            parts: Vec::new(),
            index: index.into(),
        };
        self.write_manifest(&id, &m)?;
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
        let mut m = self.manifest(id)?;
        if m.len != expected_len || m.index != expected_index {
            return Err(StoreError::Moved(m.index));
        }
        self.drop_unlisted(id, &m)?;
        if !segments.is_empty() {
            let name = format!("part.{:06}", m.parts.len());
            m.len += self.write_part(id, &name, segments)?;
            m.parts.push(name);
        }
        #[cfg(test)]
        if self.crash_before_manifest.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Other("crash before manifest".into()));
        }
        m.index = index.to_string();
        self.write_manifest(id, &m)
    }

    fn read(&self, id: &str, offset: u64, len: u64) -> Result<Box<dyn Read>, StoreError> {
        let m = self.manifest(id)?;
        let (mut skip, mut left) = (offset, len);
        let mut reader: Box<dyn Read> = Box::new(io::empty());
        for name in &m.parts {
            let path = self.root.join(id).join(name);
            let size = fs::metadata(&path).map_err(io_err)?.len();
            if skip >= size || left == 0 {
                skip = skip.saturating_sub(size);
                continue;
            }
            let mut file = File::open(path).map_err(io_err)?;
            file.seek(SeekFrom::Start(skip)).map_err(io_err)?;
            let take = (size - skip).min(left);
            reader = Box::new(reader.chain(file.take(take)));
            (skip, left) = (0, left - take);
        }
        if left > 0 {
            return Err(StoreError::NotFound);
        }
        Ok(reader)
    }

    fn delete(&self, id: &str) -> Result<(), StoreError> {
        fs::remove_dir_all(self.root.join(id)).map_err(io_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    contract_tests!(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        (DirStore::new(dir.path()), dir)
    });

    #[test]
    fn dir_store_crash_after_parts_before_manifest_recovers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DirStore::new(dir.path());
        let id = store.create("t", "i0").unwrap();
        let w = |w: &mut dyn Write| w.write_all(b"abc");
        let seg = || Segment { len: 3, write: &w };
        store.append(&id, 0, "i0", &[seg()], "i1").unwrap();
        let old_len = store.list().unwrap().packs[0].len;
        store.crash_before_manifest();
        assert!(store.append(&id, old_len, "i1", &[seg()], "i2").is_err());
        assert_eq!(store.list().unwrap().packs[0].len, old_len);
        let ok = store.append(&id, old_len, "i1", &[seg()], "i2");
        assert!(ok.is_ok(), "{ok:?}");
    }

    #[test]
    fn dir_store_list_reports_a_corrupt_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DirStore::new(dir.path());
        let id = store.create("t", "i0").unwrap();
        fs::write(dir.path().join(id).join("manifest"), "{not json").unwrap();
        assert!(store.list().is_err());
    }
}
