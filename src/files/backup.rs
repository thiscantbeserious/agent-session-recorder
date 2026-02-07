//! Backup utilities for cast files.
//!
//! Provides backup creation, restore, and path helpers used by both
//! the CLI analyze command and the TUI.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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

/// Create a backup of the given file if one doesn't already exist.
///
/// Returns `Ok(true)` if a new backup was created, `Ok(false)` if one already existed.
pub fn create_backup(path: &Path) -> Result<bool> {
    let backup = backup_path_for(path);
    if backup.exists() {
        return Ok(false);
    }
    fs::copy(path, &backup)
        .with_context(|| format!("Failed to create backup: {}", backup.display()))?;
    Ok(true)
}

/// Restore a file from its backup.
///
/// Uses an atomic temp+rename pattern for crash safety.
/// Deletes the backup file after successful restore.
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

    // Delete backup file after successful restore (best-effort, ignore errors)
    let _ = fs::remove_file(&backup);

    Ok(())
}
