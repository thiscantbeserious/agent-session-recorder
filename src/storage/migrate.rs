//! Storage migration for hidden lock and backup files.
//!
//! Scans agent directories and renames old-format files (e.g. `session.cast.lock`)
//! to hidden format (e.g. `.session.cast.lock`). Idempotent: does nothing when
//! all files are already in hidden format.

use std::fs;
use std::path::Path;

/// Result of a migration sweep.
#[derive(Debug, Default)]
pub struct StorageMigrateResult {
    pub files_renamed: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub warnings: Vec<String>,
}

/// Scan agent directories under `storage_dir` and rename old-format auxiliary files.
///
/// Old-format files have names matching `*.cast.lock` or `*.cast.bak` that do NOT
/// start with a dot. Each is renamed to its hidden equivalent (dot-prefixed).
/// If the target already exists, the old file is left in place (skipped).
///
/// Called during `StorageManager::new()` to ensure files are migrated before any
/// storage operation.
pub fn migrate_hidden_files(storage_dir: &Path) -> StorageMigrateResult {
    let mut result = StorageMigrateResult::default();

    if !storage_dir.exists() {
        return result;
    }

    let entries = match fs::read_dir(storage_dir) {
        Ok(e) => e,
        Err(_) => return result,
    };

    for entry in entries.flatten() {
        let agent_dir = entry.path();
        if !agent_dir.is_dir() {
            continue;
        }
        migrate_agent_dir(&agent_dir, &mut result);
    }

    result
}

/// Migrate auxiliary files within a single agent directory.
///
/// Renames old-format lock/backup files (no dot prefix) to hidden format (dot prefix).
fn migrate_agent_dir(agent_dir: &Path, result: &mut StorageMigrateResult) {
    let entries = match fs::read_dir(agent_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|f| f.to_str()) {
            Some(f) => f,
            None => continue,
        };

        if !is_old_format_auxiliary(filename) {
            continue;
        }

        rename_to_hidden(&path, agent_dir, filename, result);
    }
}

/// Return true if `filename` is an old-format auxiliary file (no dot prefix,
/// ends with `.cast.lock` or `.cast.bak`).
fn is_old_format_auxiliary(filename: &str) -> bool {
    if filename.starts_with('.') {
        return false;
    }
    filename.ends_with(".cast.lock") || filename.ends_with(".cast.bak")
}

/// Attempt to rename `path` to its hidden equivalent inside `agent_dir`.
fn rename_to_hidden(
    path: &Path,
    agent_dir: &Path,
    filename: &str,
    result: &mut StorageMigrateResult,
) {
    let hidden_name = format!(".{}", filename);
    let target = agent_dir.join(&hidden_name);

    if target.exists() {
        result.files_skipped += 1;
        result.warnings.push(format!(
            "Skipped {}: target {} already exists",
            path.display(),
            target.display()
        ));
        return;
    }

    match fs::rename(path, &target) {
        Ok(()) => result.files_renamed += 1,
        Err(e) => {
            result.files_failed += 1;
            result.warnings.push(format!(
                "Failed to rename {} to {}: {}",
                path.display(),
                target.display(),
                e
            ));
        }
    }
}
