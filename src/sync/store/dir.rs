//! Directory pack store: `<root>/<id>/part.NNNNNN` files plus one `manifest`.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
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

/// One directory per pack under `root`. Single writer: there is no lock here, the engine's local
/// `SyncLock` serializes writers on one machine.
///
/// Durability: part files and `manifest.tmp` are fsynced before the rename and, on unix, the
/// directory after it. There is no clean test seam for fsync, so that is covered by this comment
/// and by routing every manifest write through `write_manifest`, `write_synced` and `sync_dir`.
pub struct DirStore {
    root: PathBuf,
    // Fault hooks for the integration tests, off until a test calls the matching method.
    crash_before_manifest: AtomicBool,
    crash_before_rename: AtomicBool,
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

fn rename_retry(
    from: &Path,
    to: &Path,
    rename: &dyn Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut tries = 1;
    loop {
        match rename(from, to) {
            // Windows refuses a rename over a file an indexer or virus scanner holds open.
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && tries < 3 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            other => return other,
        }
    }
}

/// Writes `bytes` to a new file and fsyncs it, so the later rename never exposes empty data.
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Fsyncs the pack directory so the rename itself survives a power cut.
#[cfg(unix)]
fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

// Windows cannot open a directory as a file, so there is no directory fsync there.
#[cfg(not(unix))]
fn sync_dir(_dir: &Path) -> io::Result<()> {
    Ok(())
}

impl DirStore {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            crash_before_manifest: AtomicBool::new(false),
            crash_before_rename: AtomicBool::new(false),
        }
    }

    /// The next manifest write fails after `manifest.tmp` is written, before the rename.
    #[doc(hidden)]
    pub fn crash_before_rename(&self) {
        self.crash_before_rename.store(true, Ordering::SeqCst);
    }

    /// The next append writes its part file and then fails before the manifest is renamed.
    #[doc(hidden)]
    pub fn crash_before_manifest(&self) {
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
        write_synced(&tmp, text.as_bytes()).map_err(io_err)?;
        if self.crash_before_rename.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Other("crash before rename".into()));
        }
        rename_retry(&tmp, &dir.join("manifest"), &|a, b| fs::rename(a, b)).map_err(io_err)?;
        sync_dir(&dir).map_err(io_err)
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
        file.sync_all().map_err(io_err)?;
        Ok(file.metadata().map_err(io_err)?.len())
    }
}

impl DirStore {
    /// The first free `pack-N`, taken by `create_dir` itself so a racing writer never shares it.
    fn make_pack_dir(&self) -> Result<String, StoreError> {
        fs::create_dir_all(&self.root).map_err(io_err)?;
        for n in 1.. {
            let id = format!("pack-{n}");
            match fs::create_dir(self.root.join(&id)) {
                Ok(()) => return Ok(id),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(io_err(e)),
            }
        }
        unreachable!("the counter does not run out")
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
            // a stray file such as .DS_Store is no pack
            if !e.path().is_dir() {
                continue;
            }
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
                files: m.parts.len(),
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
        let id = self.make_pack_dir()?;
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
        if self.crash_before_manifest.swap(false, Ordering::SeqCst) {
            return Err(StoreError::Other("crash before manifest".into()));
        }
        m.index = index.to_string();
        self.write_manifest(id, &m)
    }

    fn files_for(&self, lens: &[u64]) -> usize {
        usize::from(!lens.is_empty())
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
    use super::{rename_retry, DirStore, PackStore, Segment};
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;

    /// `exists` follows the link and says no, `create_dir` says AlreadyExists, as a racing writer would.
    #[cfg(unix)]
    #[test]
    fn create_never_reuses_an_existing_pack_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::os::unix::fs::symlink(dir.path().join("nowhere"), dir.path().join("pack-1")).unwrap();
        let store = DirStore::new(dir.path());
        assert_eq!(store.create("t", "i0").unwrap(), "pack-2");
    }

    #[test]
    fn dir_list_ignores_a_stray_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(".DS_Store"), b"x").unwrap();
        let store = DirStore::new(dir.path());
        assert!(store.list().is_ok());
    }

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
    fn dir_store_next_append_deletes_unlisted_parts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DirStore::new(dir.path());
        let id = store.create("t", "i0").unwrap();
        let w = |w: &mut dyn Write| w.write_all(b"abc");
        let seg = Segment { len: 3, write: &w };
        store.crash_before_manifest();
        assert!(store.append(&id, 0, "i0", &[seg], "i1").is_err());
        let orphan = dir.path().join(&id).join("part.000000");
        assert!(orphan.exists());
        store.append(&id, 0, "i0", &[], "i1").unwrap();
        assert!(!orphan.exists());
    }

    #[test]
    fn dir_store_crash_before_the_rename_keeps_the_old_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = DirStore::new(dir.path());
        let id = store.create("t", "i0").unwrap();
        let w = |w: &mut dyn Write| w.write_all(b"abc");
        let seg = || Segment { len: 3, write: &w };
        store.crash_before_rename();
        assert!(store.append(&id, 0, "i0", &[seg()], "i1").is_err());
        assert_eq!(store.list().unwrap().packs[0].len, 0);
        assert_eq!(store.read_index(&id).unwrap(), "i0");
        store.append(&id, 0, "i0", &[seg()], "i1").unwrap();
        assert_eq!(store.list().unwrap().packs[0].len, 3);
    }

    #[test]
    fn rename_retry_survives_two_permission_denied() {
        let calls = std::cell::Cell::new(0);
        let flaky = |_: &Path, _: &Path| {
            calls.set(calls.get() + 1);
            if calls.get() <= 2 {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            Ok(())
        };
        assert!(rename_retry(Path::new("a"), Path::new("b"), &flaky).is_ok());
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn rename_retry_gives_up_on_other_errors_at_once() {
        let calls = std::cell::Cell::new(0);
        let broken = |_: &Path, _: &Path| {
            calls.set(calls.get() + 1);
            Err(io::Error::from(io::ErrorKind::NotFound))
        };
        assert!(rename_retry(Path::new("a"), Path::new("b"), &broken).is_err());
        assert_eq!(calls.get(), 1);
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
