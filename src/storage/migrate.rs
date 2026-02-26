//! Storage migrations for the agent session recorder.
//!
//! Migrations run on every `StorageManager::new()` call. Each is idempotent:
//! when there is nothing to do, the cost is one `read_dir` per agent directory.
//! No version file is needed — idempotency is the version check.

use std::fs;
use std::path::Path;

/// Result of running all storage migrations.
#[derive(Debug, Default)]
pub struct MigrateResult {
    pub files_renamed: usize,
    pub files_skipped: usize,
    pub files_failed: usize,
    pub warnings: Vec<String>,
}

/// Run all storage migrations in declaration order. Each is idempotent.
///
/// Called during `StorageManager::new()` to ensure files are migrated before any
/// storage operation.
pub fn execute(storage_dir: &Path) -> MigrateResult {
    let mut result = MigrateResult::default();
    if !storage_dir.exists() {
        return result;
    }

    v1_hidden_auxiliary_files(storage_dir, &mut result);
    // Future migrations go here in order:
    // v2_something(storage_dir, &mut result);

    result
}

/// V1: Rename old-format auxiliary files to hidden dot-prefixed format.
///
/// Scans agent directories for files matching `*.cast.lock` or `*.cast.bak`
/// whose filename does not start with a dot, and renames them to the hidden
/// equivalent (dot-prefixed).
fn v1_hidden_auxiliary_files(storage_dir: &Path, result: &mut MigrateResult) {
    let entries = match fs::read_dir(storage_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let agent_dir = entry.path();
        if !agent_dir.is_dir() {
            continue;
        }
        migrate_agent_dir(&agent_dir, result);
    }
}

/// Migrate auxiliary files within a single agent directory.
///
/// Renames old-format lock/backup files (no dot prefix) to hidden format (dot prefix).
fn migrate_agent_dir(agent_dir: &Path, result: &mut MigrateResult) {
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
fn rename_to_hidden(path: &Path, agent_dir: &Path, filename: &str, result: &mut MigrateResult) {
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
