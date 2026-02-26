//! Tests for the storage migration sweep that renames old-format auxiliary files
//! to hidden format.

use agr::storage::migrate::migrate_hidden_files;
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a directory structure: `<storage>/<agent>/` and return the agent dir.
fn make_agent_dir(storage: &TempDir, agent: &str) -> std::path::PathBuf {
    let agent_dir = storage.path().join(agent);
    fs::create_dir_all(&agent_dir).unwrap();
    agent_dir
}

/// Write an empty file at `dir/filename`.
fn touch(dir: &std::path::Path, filename: &str) {
    fs::write(dir.join(filename), b"").unwrap();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn migrate_renames_old_format_lock_files() {
    let storage = TempDir::new().unwrap();
    let agent_dir = make_agent_dir(&storage, "claude");
    touch(&agent_dir, "session.cast.lock");

    let result = migrate_hidden_files(storage.path());

    assert_eq!(result.files_renamed, 1);
    assert_eq!(result.files_failed, 0);
    assert!(!agent_dir.join("session.cast.lock").exists());
    assert!(agent_dir.join(".session.cast.lock").exists());
}

#[test]
fn migrate_renames_old_format_backup_files() {
    let storage = TempDir::new().unwrap();
    let agent_dir = make_agent_dir(&storage, "claude");
    touch(&agent_dir, "session.cast.bak");

    let result = migrate_hidden_files(storage.path());

    assert_eq!(result.files_renamed, 1);
    assert_eq!(result.files_failed, 0);
    assert!(!agent_dir.join("session.cast.bak").exists());
    assert!(agent_dir.join(".session.cast.bak").exists());
}

#[test]
fn migrate_skips_already_hidden_files() {
    let storage = TempDir::new().unwrap();
    let agent_dir = make_agent_dir(&storage, "claude");
    touch(&agent_dir, ".session.cast.lock");

    let result = migrate_hidden_files(storage.path());

    assert_eq!(result.files_renamed, 0);
    assert_eq!(result.files_skipped, 0);
    assert_eq!(result.files_failed, 0);
    assert!(agent_dir.join(".session.cast.lock").exists());
}

#[test]
fn migrate_skips_when_target_exists() {
    let storage = TempDir::new().unwrap();
    let agent_dir = make_agent_dir(&storage, "claude");
    // Both old and new format exist
    touch(&agent_dir, "session.cast.lock");
    touch(&agent_dir, ".session.cast.lock");

    let result = migrate_hidden_files(storage.path());

    assert_eq!(result.files_renamed, 0);
    assert_eq!(result.files_skipped, 1);
    // Old file is left in place
    assert!(agent_dir.join("session.cast.lock").exists());
    assert!(agent_dir.join(".session.cast.lock").exists());
}

#[test]
fn migrate_is_idempotent() {
    let storage = TempDir::new().unwrap();
    let agent_dir = make_agent_dir(&storage, "claude");
    touch(&agent_dir, "session.cast.lock");

    // First run migrates
    let first = migrate_hidden_files(storage.path());
    assert_eq!(first.files_renamed, 1);

    // Second run does nothing
    let second = migrate_hidden_files(storage.path());
    assert_eq!(second.files_renamed, 0);
    assert_eq!(second.files_failed, 0);
    assert_eq!(second.files_skipped, 0);
}

#[test]
fn migrate_handles_empty_storage_directory() {
    let storage = TempDir::new().unwrap();
    // Create an agent directory that is empty
    make_agent_dir(&storage, "claude");

    let result = migrate_hidden_files(storage.path());

    assert_eq!(result.files_renamed, 0);
    assert_eq!(result.files_failed, 0);
    assert_eq!(result.files_skipped, 0);
}

#[test]
fn migrate_handles_nonexistent_storage_directory() {
    let storage = TempDir::new().unwrap();
    let nonexistent = storage.path().join("does_not_exist");

    let result = migrate_hidden_files(&nonexistent);

    assert_eq!(result.files_renamed, 0);
    assert_eq!(result.files_failed, 0);
    assert_eq!(result.files_skipped, 0);
}
