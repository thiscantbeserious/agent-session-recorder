//! Orphaned backup and stale lock detection and removal

use super::helpers::storage::create_test_session;
use agr::files::{backup, lock};
use agr::storage::orphans;
use agr::StorageManager;
use std::fs;
use tempfile::TempDir;

#[test]
fn backup_of_missing_session_is_orphaned_either_suffix() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path().join("a");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".x.cast.bak"), "12345").unwrap();
    fs::write(dir.join(".y.cast.gz.bak"), "123").unwrap();
    let found = orphans::find(temp.path()).unwrap();
    assert_eq!(found.backups.len(), 2, "{found:?}");
    assert_eq!(found.bytes, 8);
}

#[test]
fn backup_of_existing_session_in_either_variant_is_owned() {
    let temp = TempDir::new().unwrap();
    let plain = create_test_session(temp.path(), "a", "x.cast", "c");
    fs::write(backup::backup_path_for(&plain), "b").unwrap();
    let gz = create_test_session(temp.path(), "a", "y.cast.gz", "c");
    fs::write(backup::backup_path_for(&gz), "b").unwrap();
    // plain backup left behind after the session became .cast.gz
    fs::write(temp.path().join("a/.y.cast.bak"), "b").unwrap();
    assert!(orphans::find(temp.path()).unwrap().is_empty());
}

#[test]
fn stale_lock_is_listed_and_live_lock_is_not() {
    let temp = TempDir::new().unwrap();
    let live = create_test_session(temp.path(), "a", "live.cast", "c");
    lock::create_lock(&live).unwrap();
    let dir = temp.path().join("a");
    fs::write(
        dir.join(".dead.cast.lock"),
        r#"{"pid":4294967,"started":"x"}"#,
    )
    .unwrap();
    fs::write(dir.join(".junk.cast.lock"), "not json").unwrap();
    let found = orphans::find(temp.path()).unwrap();
    assert_eq!(found.locks.len(), 2, "{found:?}");
    assert!(found.locks.iter().all(|p| !p.ends_with(".live.cast.lock")));
    assert!(found.backups.is_empty());
}

#[test]
fn remove_deletes_only_the_listed_files() {
    let temp = TempDir::new().unwrap();
    let keep = create_test_session(temp.path(), "a", "x.cast", "c");
    fs::write(backup::backup_path_for(&keep), "b").unwrap();
    let dir = temp.path().join("a");
    fs::write(dir.join(".gone.cast.bak"), "b").unwrap();
    fs::write(dir.join(".dead.cast.lock"), "junk").unwrap();
    let found = orphans::find(temp.path()).unwrap();
    assert_eq!(orphans::remove(&found), 2);
    assert!(orphans::find(temp.path()).unwrap().is_empty());
    assert!(keep.exists() && backup::backup_path_for(&keep).exists());
    assert!(!dir.join(".gone.cast.bak").exists());
}

#[test]
fn missing_storage_dir_is_empty() {
    let temp = TempDir::new().unwrap();
    assert!(orphans::find(&temp.path().join("nope")).unwrap().is_empty());
}

fn gz_manager() -> (TempDir, StorageManager, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let gz = create_test_session(temp.path(), "a", "x.cast.gz", "c");
    fs::write(backup::backup_path_for(&gz), "b").unwrap();
    let m = StorageManager::new(super::helpers::storage::create_test_config(&temp));
    (temp, m, gz)
}

#[test]
fn delete_sessions_of_gzip_leaves_no_orphan_backup() {
    let (temp, m, gz) = gz_manager();
    let sessions = m.list_sessions(None).unwrap();
    m.delete_sessions(&sessions).unwrap();
    assert!(!gz.exists());
    assert!(orphans::find(temp.path()).unwrap().is_empty());
    assert!(!backup::backup_path_for(&gz).exists());
}

#[test]
fn rename_of_gzip_moves_the_backup_along() {
    let (temp, _m, gz) = gz_manager();
    let new = agr::files::filename::rename_file(&gz, "renamed").unwrap();
    assert!(new.ends_with("renamed.cast.gz"));
    assert!(backup::backup_path_for(&new).exists());
    assert!(orphans::find(temp.path()).unwrap().is_empty());
}
