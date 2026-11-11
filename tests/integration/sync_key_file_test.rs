//! Key file creation through the public key module.

use agr::sync::key::{save_new, KeyError};
use agr::sync::Key;

#[test]
fn save_new_refuses_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.key");
    save_new(&path, &Key::generate()).unwrap();
    assert_eq!(save_new(&path, &Key::generate()), Err(KeyError::Exists));
}

#[cfg(unix)]
#[test]
fn save_new_uses_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sync.key");
    save_new(&path, &Key::generate()).unwrap();
    assert!(path.exists());
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}
