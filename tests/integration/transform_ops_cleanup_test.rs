//! Tests for restore, temp file cleanup and the transform/restore round trip

use super::transform_ops_test::create_test_cast_file;
use agr::asciicast::{apply_transforms, Event};
use agr::files::backup::{backup_path_for, has_backup, restore_from_backup};
use agr::files::codec;
use std::fs;
use tempfile::TempDir;

// ========================================================================
// restore_from_backup tests
// ========================================================================

#[test]
fn restore_from_backup_restores_original_content() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![Event::output(0.1, "hello"), Event::output(10.0, "world")],
    );

    // Get original bytes
    let original_bytes = fs::read(&path).unwrap();

    // Transform (creates backup)
    apply_transforms(&path).unwrap();

    // File should now be different
    let transformed_bytes = fs::read(&path).unwrap();
    assert_ne!(original_bytes, transformed_bytes);

    // Restore
    restore_from_backup(&path).unwrap();

    // Should match original
    let restored_bytes = fs::read(&path).unwrap();
    assert_eq!(original_bytes, restored_bytes);
}

#[test]
fn restore_from_backup_fails_when_no_backup() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(&dir, "test.cast", vec![Event::output(0.1, "hello")]);

    let result = restore_from_backup(&path);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No backup exists"));
}

// ========================================================================
// Temp file cleanup tests
// ========================================================================

#[test]
fn apply_transforms_no_temp_file_left_on_success() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![Event::output(0.1, "hello"), Event::output(5.0, "world")],
    );

    // Apply transforms
    apply_transforms(&path).unwrap();

    // Verify no .tmp file exists after successful operation
    let temp_path = codec::temp_path_for(&path);
    assert!(
        !temp_path.exists(),
        "Temp file should not exist after successful transform"
    );
}

#[test]
fn restore_from_backup_no_temp_file_left_on_success() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![Event::output(0.1, "hello"), Event::output(5.0, "world")],
    );

    // Create backup and transform
    apply_transforms(&path).unwrap();

    // Restore
    restore_from_backup(&path).unwrap();

    // Verify no .tmp file exists after successful operation
    let temp_path = codec::temp_path_for(&path);
    assert!(
        !temp_path.exists(),
        "Temp file should not exist after successful restore"
    );
}

#[test]
fn restore_from_backup_deletes_backup_file() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![Event::output(0.1, "hello"), Event::output(5.0, "world")],
    );

    // Create backup via transform
    apply_transforms(&path).unwrap();
    let backup_path = backup_path_for(&path);
    assert!(backup_path.exists(), "Backup should exist after transform");

    // Restore from backup
    restore_from_backup(&path).unwrap();

    // Verify backup is deleted after restore
    assert!(
        !backup_path.exists(),
        "Backup file should be deleted after successful restore"
    );

    // Verify has_backup returns false
    assert!(
        !has_backup(&path),
        "has_backup should return false after restore"
    );
}

#[cfg(unix)]
#[test]
fn apply_transforms_cleans_up_temp_file_on_rename_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![Event::output(0.1, "hello"), Event::output(5.0, "world")],
    );

    // Make the directory read-only to cause rename to fail
    let dir_path = dir.path();
    let original_perms = fs::metadata(dir_path).unwrap().permissions();
    let mut readonly_perms = original_perms.clone();
    readonly_perms.set_mode(0o555); // read + execute only, no write
    fs::set_permissions(dir_path, readonly_perms).unwrap();

    // Try to apply transforms - should fail because rename cannot write
    let result = apply_transforms(&path);

    // Restore permissions before assertions (so TempDir can clean up)
    fs::set_permissions(dir_path, original_perms).unwrap();

    // The operation should have failed
    assert!(
        result.is_err(),
        "Expected transform to fail with read-only directory"
    );

    // Verify no .tmp file exists (cleanup should have happened)
    let temp_path = codec::temp_path_for(&path);
    assert!(
        !temp_path.exists(),
        "Temp file should be cleaned up after rename failure"
    );
}

#[cfg(unix)]
#[test]
fn restore_from_backup_cleans_up_temp_file_on_rename_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![Event::output(0.1, "hello"), Event::output(5.0, "world")],
    );

    // Apply transforms to create a backup
    apply_transforms(&path).unwrap();

    // Make the directory read-only to cause rename to fail
    let dir_path = dir.path();
    let original_perms = fs::metadata(dir_path).unwrap().permissions();
    let mut readonly_perms = original_perms.clone();
    readonly_perms.set_mode(0o555); // read + execute only, no write
    fs::set_permissions(dir_path, readonly_perms).unwrap();

    // Try to restore - should fail because rename cannot write
    let result = restore_from_backup(&path);

    // Restore permissions before assertions (so TempDir can clean up)
    fs::set_permissions(dir_path, original_perms).unwrap();

    // The operation should have failed
    assert!(
        result.is_err(),
        "Expected restore to fail with read-only directory"
    );

    // Verify no .tmp file exists (cleanup should have happened)
    let temp_path = codec::temp_path_for(&path);
    assert!(
        !temp_path.exists(),
        "Temp file should be cleaned up after rename failure"
    );
}

// ========================================================================
// Round-trip integrity test (critical for Stage 3)
// ========================================================================

#[test]
fn round_trip_transform_restore_preserves_original() {
    let dir = TempDir::new().unwrap();
    let path = create_test_cast_file(
        &dir,
        "test.cast",
        vec![
            Event::output(0.1, "hello"),
            Event::output(10.0, "world"),
            Event::marker(0.5, "test marker"),
            Event::output(5.0, "end"),
        ],
    );

    // Store original bytes
    let original_bytes = fs::read(&path).unwrap();

    // Transform 1 - should create backup
    let result1 = apply_transforms(&path).unwrap();
    assert!(result1.backup_created);

    // Restore - deletes backup
    restore_from_backup(&path).unwrap();
    assert!(!has_backup(&path), "Backup should be deleted after restore");

    // Transform 2 - should create NEW backup (previous was deleted by restore)
    let result2 = apply_transforms(&path).unwrap();
    assert!(
        result2.backup_created,
        "New backup should be created since previous was deleted"
    );

    // Restore again - deletes backup again
    restore_from_backup(&path).unwrap();
    assert!(
        !has_backup(&path),
        "Backup should be deleted after second restore"
    );

    // Final bytes should match original
    let final_bytes = fs::read(&path).unwrap();
    assert_eq!(
        original_bytes, final_bytes,
        "Round-trip did not preserve original file"
    );
}
