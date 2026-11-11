//! Sync key file, `agr-sync-key v1 <base32>`, mode 0600 on unix.

use std::io::Write;
use std::path::{Path, PathBuf};

use data_encoding::BASE32_NOPAD;
use thiserror::Error;

use crate::sync::crypto::Key;

/// Errors from reading or writing the key file, never carry key bytes.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KeyError {
    #[error("sync key file {0} is readable by others, run `chmod 600 {0}`")]
    Insecure(PathBuf),
    #[error("sync key file is not valid")]
    Malformed,
    #[error("a sync key already exists")]
    Exists,
    #[error("a different sync key already exists, use --force to replace it")]
    Differs,
    #[error("sync key file error: {0}")]
    Io(String),
}

/// What `import` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Imported {
    Created,
    Same,
    Replaced,
}

const KEY_PREFIX: &str = "agr-sync-key v1 ";

/// Load the key, `None` when the file is missing.
pub(super) fn load(path: &Path) -> Result<Option<Key>, KeyError> {
    if !path.exists() {
        return Ok(None);
    }
    check_mode(path)?;
    let text = std::fs::read_to_string(path).map_err(|e| KeyError::Io(e.kind().to_string()))?;
    parse(&text).map(Some)
}

fn parse(text: &str) -> Result<Key, KeyError> {
    let body = text
        .trim()
        .strip_prefix(KEY_PREFIX)
        .ok_or(KeyError::Malformed)?;
    let bytes = BASE32_NOPAD
        .decode(body.as_bytes())
        .map_err(|_| KeyError::Malformed)?;
    let array: [u8; 32] = bytes.try_into().map_err(|_| KeyError::Malformed)?;
    Ok(Key::from_bytes(array))
}

#[cfg(unix)]
fn check_mode(path: &Path) -> Result<(), KeyError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)
        .map_err(|e| KeyError::Io(e.kind().to_string()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(KeyError::Insecure(path.to_path_buf()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_mode(_path: &Path) -> Result<(), KeyError> {
    Ok(())
}

/// Create the key file with mode 0600, fails when it exists.
pub fn save_new(path: &Path, key: &Key) -> Result<(), KeyError> {
    if path.exists() {
        return Err(KeyError::Exists);
    }
    write_private(path, key)
}

/// Write via a private temp file and rename, the key never exists with loose bits.
fn write_private(path: &Path, key: &Key) -> Result<(), KeyError> {
    let io = |e: std::io::Error| KeyError::Io(e.kind().to_string());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    let tmp = crate::files::codec::temp_path_for(path);
    let _ = std::fs::remove_file(&tmp);
    let line = format!("{KEY_PREFIX}{}\n", BASE32_NOPAD.encode(key.as_bytes()));
    let written = open_private(&tmp).and_then(|mut f| {
        f.write_all(line.as_bytes())?;
        f.sync_all()
    });
    let result = written.and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(io)
}

#[cfg(unix)]
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Store an imported key, a different existing key needs `force`.
pub(super) fn import(path: &Path, key: &Key, force: bool) -> Result<Imported, KeyError> {
    let Some(existing) = load(path)? else {
        write_private(path, key)?;
        return Ok(Imported::Created);
    };
    if existing.as_bytes() == key.as_bytes() {
        return Ok(Imported::Same);
    }
    if !force {
        return Err(KeyError::Differs);
    }
    write_private(path, key)?;
    Ok(Imported::Replaced)
}

// inline: load and import are private to the app layer, the tests read the key file back through them
#[cfg(test)]
mod tests {
    use super::{import, load, save_new, Imported, Key, KeyError, Path};

    fn loaded(path: &Path) -> Option<[u8; 32]> {
        load(path).unwrap().map(|k| *k.as_bytes())
    }

    #[test]
    fn missing_file_loads_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(&dir.path().join("sync.key")).unwrap().is_none());
    }

    #[test]
    fn save_new_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.key");
        let key = Key::generate();
        save_new(&path, &key).unwrap();
        assert_eq!(loaded(&path), Some(*key.as_bytes()));
    }

    #[cfg(unix)]
    #[test]
    fn load_refuses_world_readable_with_chmod_hint() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.key");
        save_new(&path, &Key::generate()).unwrap();
        assert!(path.exists());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = load(&path).err();
        assert!(matches!(err, Some(KeyError::Insecure(_))), "{err:?}");
        assert!(err.unwrap().to_string().contains("chmod 600"));
    }

    #[test]
    fn malformed_file_is_reported_without_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.key");
        save_new(&path, &Key::generate()).unwrap();
        assert!(path.exists());
        crate::files::codec::atomic_write_raw(&path, b"garbage").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(load(&path).err(), Some(KeyError::Malformed));
    }

    #[test]
    fn import_creates_then_accepts_same_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.key");
        let key = Key::generate();
        assert_eq!(import(&path, &key, false), Ok(Imported::Created));
        assert_eq!(loaded(&path), Some(*key.as_bytes()));
        assert_eq!(import(&path, &key, false), Ok(Imported::Same));
    }

    #[test]
    fn import_refuses_different_key_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.key");
        let old = Key::generate();
        save_new(&path, &old).unwrap();
        assert_eq!(
            import(&path, &Key::generate(), false),
            Err(KeyError::Differs)
        );
        assert_eq!(loaded(&path), Some(*old.as_bytes()));
    }

    #[test]
    fn import_force_replaces_and_keeps_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync.key");
        save_new(&path, &Key::generate()).unwrap();
        let new = Key::generate();
        assert_eq!(import(&path, &new, true), Ok(Imported::Replaced));
        assert_eq!(loaded(&path), Some(*new.as_bytes()));
    }
}
