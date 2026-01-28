//! High-level transform operations for asciicast files.
//!
//! This module provides file-level transform functions including backup management,
//! transform application, and restore operations. These are higher-level operations
//! built on top of the core transform traits.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{AsciicastFile, SilenceRemoval, Transform, DEFAULT_SILENCE_THRESHOLD};

/// Result of applying transforms to a recording.
#[derive(Debug, Clone)]
pub struct TransformResult {
    /// Original duration before transform (seconds).
    pub original_duration: f64,
    /// New duration after transform (seconds).
    pub new_duration: f64,
    /// Path to the backup file (if created).
    pub backup_path: Option<PathBuf>,
    /// Whether a new backup was created (vs using existing).
    pub backup_created: bool,
}

impl TransformResult {
    /// Calculate time saved by the transform.
    pub fn time_saved(&self) -> f64 {
        self.original_duration - self.new_duration
    }

    /// Calculate percentage of time saved.
    pub fn percent_saved(&self) -> f64 {
        if self.original_duration > 0.0 {
            (self.time_saved() / self.original_duration) * 100.0
        } else {
            0.0
        }
    }
}

/// Get the backup path for a given file.
///
/// The backup path is the original path with `.bak` appended.
pub fn backup_path_for(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    PathBuf::from(backup)
}

/// Check if a backup exists for the given file.
pub fn has_backup(path: &Path) -> bool {
    backup_path_for(path).exists()
}

/// Apply all transforms to a recording file.
///
/// This function:
/// 1. Creates a backup if one doesn't already exist
/// 2. Parses the file
/// 3. Applies silence removal with threshold from header or default
/// 4. Writes the modified file back
///
/// # Arguments
///
/// * `path` - Path to the `.cast` file to transform
///
/// # Returns
///
/// Returns a `TransformResult` with duration information and backup status.
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be read or parsed
/// - Backup creation fails
/// - Writing the transformed file fails
pub fn apply_transforms(path: &Path) -> Result<TransformResult> {
    // Parse the file first to get original duration
    let mut cast = AsciicastFile::parse(path)
        .with_context(|| format!("Failed to parse asciicast file: {}", path.display()))?;

    let original_duration = cast.duration();

    // Create backup if it doesn't exist
    let backup = backup_path_for(path);
    let backup_created = if !backup.exists() {
        fs::copy(path, &backup)
            .with_context(|| format!("Failed to create backup: {}", backup.display()))?;
        true
    } else {
        false
    };

    // Resolve threshold: header's idle_time_limit or default
    let threshold = cast
        .header
        .idle_time_limit
        .unwrap_or(DEFAULT_SILENCE_THRESHOLD);

    // Apply silence removal transform
    let mut transform = SilenceRemoval::new(threshold);
    transform.transform(&mut cast.events);

    let new_duration = cast.duration();

    // Write to temp file first, then atomically rename to prevent data corruption
    // if write fails mid-operation (disk full, permissions issue, crash)
    let temp_path = path.with_extension("cast.tmp");
    cast.write(&temp_path)
        .with_context(|| format!("Failed to write transformed file: {}", temp_path.display()))?;

    if let Err(e) = fs::rename(&temp_path, path) {
        // Clean up temp file on failure (best-effort, ignore cleanup errors)
        let _ = fs::remove_file(&temp_path);
        return Err(e)
            .with_context(|| format!("Failed to replace original file: {}", path.display()));
    }

    Ok(TransformResult {
        original_duration,
        new_duration,
        backup_path: Some(backup),
        backup_created,
    })
}

/// Restore a file from its backup.
///
/// # Arguments
///
/// * `path` - Path to the `.cast` file to restore
///
/// # Returns
///
/// Returns `Ok(())` if restore succeeded.
///
/// # Errors
///
/// Returns an error if:
/// - No backup exists for the file
/// - The backup cannot be read
/// - Writing the restored file fails
pub fn restore_from_backup(path: &Path) -> Result<()> {
    let backup = backup_path_for(path);

    if !backup.exists() {
        anyhow::bail!("No backup exists for: {}", path.display());
    }

    // Use atomic temp+rename pattern for crash safety
    let temp_path = path.with_extension("cast.tmp");

    fs::copy(&backup, &temp_path)
        .with_context(|| format!("Failed to copy backup to temp file: {}", backup.display()))?;

    if let Err(e) = fs::rename(&temp_path, path) {
        // Clean up temp file on failure
        let _ = fs::remove_file(&temp_path);
        return Err(e)
            .with_context(|| format!("Failed to restore from backup: {}", path.display()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asciicast::{Event, Header};
    use std::io::Write;
    use tempfile::TempDir;

    // ========================================================================
    // Helper Functions
    // ========================================================================

    fn create_test_cast_file(dir: &TempDir, name: &str, events: Vec<Event>) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = AsciicastFile::new(Header {
            version: 3,
            width: Some(80),
            height: Some(24),
            term: None,
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            env: None,
            idle_time_limit: None,
        });
        file.events = events;
        file.write(&path).unwrap();
        path
    }

    fn create_test_cast_with_idle_limit(
        dir: &TempDir,
        name: &str,
        idle_time_limit: f64,
        events: Vec<Event>,
    ) -> PathBuf {
        let path = dir.path().join(name);
        let mut file = AsciicastFile::new(Header {
            version: 3,
            width: Some(80),
            height: Some(24),
            term: None,
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            env: None,
            idle_time_limit: Some(idle_time_limit),
        });
        file.events = events;
        file.write(&path).unwrap();
        path
    }

    // ========================================================================
    // backup_path_for tests
    // ========================================================================

    #[test]
    fn backup_path_appends_bak_extension() {
        let path = Path::new("/some/path/recording.cast");
        let backup = backup_path_for(path);
        assert_eq!(backup, PathBuf::from("/some/path/recording.cast.bak"));
    }

    #[test]
    fn backup_path_handles_relative_path() {
        let path = Path::new("recording.cast");
        let backup = backup_path_for(path);
        assert_eq!(backup, PathBuf::from("recording.cast.bak"));
    }

    // ========================================================================
    // has_backup tests
    // ========================================================================

    #[test]
    fn has_backup_returns_false_when_no_backup() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_file(&dir, "test.cast", vec![Event::output(0.1, "hello")]);

        assert!(!has_backup(&path));
    }

    #[test]
    fn has_backup_returns_true_when_backup_exists() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_file(&dir, "test.cast", vec![Event::output(0.1, "hello")]);

        // Create backup manually
        let backup = backup_path_for(&path);
        fs::copy(&path, &backup).unwrap();

        assert!(has_backup(&path));
    }

    // ========================================================================
    // apply_transforms tests
    // ========================================================================

    #[test]
    fn apply_transforms_creates_backup_when_none_exists() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_file(
            &dir,
            "test.cast",
            vec![
                Event::output(0.1, "hello"),
                Event::output(5.0, "world"), // 5s gap will be reduced
            ],
        );

        let result = apply_transforms(&path).unwrap();

        assert!(result.backup_created);
        assert!(result.backup_path.is_some());
        assert!(has_backup(&path));
    }

    #[test]
    fn apply_transforms_does_not_overwrite_existing_backup() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_file(
            &dir,
            "test.cast",
            vec![
                Event::output(0.1, "original"),
                Event::output(5.0, "content"),
            ],
        );

        // Create initial backup with known content
        let backup = backup_path_for(&path);
        let mut backup_file = fs::File::create(&backup).unwrap();
        backup_file.write_all(b"ORIGINAL_BACKUP").unwrap();

        let result = apply_transforms(&path).unwrap();

        // Should not have created new backup
        assert!(!result.backup_created);

        // Backup should still have original content
        let backup_content = fs::read_to_string(&backup).unwrap();
        assert_eq!(backup_content, "ORIGINAL_BACKUP");
    }

    #[test]
    fn apply_transforms_reduces_duration_with_silence() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_file(
            &dir,
            "test.cast",
            vec![
                Event::output(0.1, "hello"),
                Event::output(10.0, "after pause"), // 10s gap exceeds 2.0s threshold
                Event::output(0.2, "end"),
            ],
        );

        let result = apply_transforms(&path).unwrap();

        // Original: 0.1 + 10.0 + 0.2 = 10.3s
        assert!((result.original_duration - 10.3).abs() < 0.001);

        // New: 0.1 + 2.0 + 0.2 = 2.3s (10s capped to 2s)
        assert!((result.new_duration - 2.3).abs() < 0.001);

        // Time saved: 8.0s
        assert!((result.time_saved() - 8.0).abs() < 0.001);
    }

    #[test]
    fn apply_transforms_uses_header_idle_time_limit() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_with_idle_limit(
            &dir,
            "test.cast",
            1.0, // Use 1.0s threshold from header
            vec![
                Event::output(0.1, "hello"),
                Event::output(3.0, "after pause"), // 3s gap exceeds 1.0s threshold
                Event::output(0.2, "end"),
            ],
        );

        let result = apply_transforms(&path).unwrap();

        // Original: 0.1 + 3.0 + 0.2 = 3.3s
        assert!((result.original_duration - 3.3).abs() < 0.001);

        // New: 0.1 + 1.0 + 0.2 = 1.3s (3s capped to 1s from header)
        assert!((result.new_duration - 1.3).abs() < 0.001);
    }

    #[test]
    fn apply_transforms_no_change_when_no_silence() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_file(
            &dir,
            "test.cast",
            vec![
                Event::output(0.1, "fast"),
                Event::output(0.2, "typing"),
                Event::output(0.1, "here"),
            ],
        );

        let result = apply_transforms(&path).unwrap();

        // No gaps exceed threshold, so duration unchanged
        assert!((result.original_duration - result.new_duration).abs() < 0.001);
        assert!((result.time_saved()).abs() < 0.001);
    }

    #[test]
    fn apply_transforms_modifies_file_in_place() {
        let dir = TempDir::new().unwrap();
        let path = create_test_cast_file(
            &dir,
            "test.cast",
            vec![
                Event::output(0.1, "hello"),
                Event::output(10.0, "world"), // Will be capped to 2.0
            ],
        );

        apply_transforms(&path).unwrap();

        // Re-read the file and check duration
        let modified = AsciicastFile::parse(&path).unwrap();
        assert!((modified.duration() - 2.1).abs() < 0.001); // 0.1 + 2.0
    }

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
    // TransformResult tests
    // ========================================================================

    #[test]
    fn transform_result_time_saved() {
        let result = TransformResult {
            original_duration: 100.0,
            new_duration: 30.0,
            backup_path: None,
            backup_created: false,
        };

        assert!((result.time_saved() - 70.0).abs() < 0.001);
    }

    #[test]
    fn transform_result_percent_saved() {
        let result = TransformResult {
            original_duration: 100.0,
            new_duration: 30.0,
            backup_path: None,
            backup_created: false,
        };

        assert!((result.percent_saved() - 70.0).abs() < 0.001);
    }

    #[test]
    fn transform_result_percent_saved_zero_duration() {
        let result = TransformResult {
            original_duration: 0.0,
            new_duration: 0.0,
            backup_path: None,
            backup_created: false,
        };

        assert!((result.percent_saved()).abs() < 0.001);
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
        let temp_path = path.with_extension("cast.tmp");
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
        let temp_path = path.with_extension("cast.tmp");
        assert!(
            !temp_path.exists(),
            "Temp file should not exist after successful restore"
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
        let temp_path = path.with_extension("cast.tmp");
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
        let temp_path = path.with_extension("cast.tmp");
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

        // Restore
        restore_from_backup(&path).unwrap();

        // Transform 2 - should NOT create new backup (existing preserved)
        let result2 = apply_transforms(&path).unwrap();
        assert!(!result2.backup_created);

        // Restore again
        restore_from_backup(&path).unwrap();

        // Final bytes should match original
        let final_bytes = fs::read(&path).unwrap();
        assert_eq!(
            original_bytes, final_bytes,
            "Round-trip did not preserve original file"
        );
    }
}
