//! File-related utilities for agent session recordings.

pub mod backup;
pub mod filename;
pub mod lock;
pub mod resolve;
pub mod template;

/// Remove lock and backup auxiliary files in both old and new formats.
///
/// Called from all session deletion paths to ensure no orphaned lock or backup files
/// remain after a `.cast` file is removed. All removals are best-effort: errors
/// (including `NotFound`) are silently ignored.
pub fn remove_auxiliary_files(path: &std::path::Path) {
    let _ = std::fs::remove_file(lock::lock_path_for(path));
    let _ = std::fs::remove_file(lock::old_lock_path_for(path));
    let _ = std::fs::remove_file(backup::backup_path_for(path));
    let _ = std::fs::remove_file(backup::old_backup_path_for(path));
}
