//! File-related utilities for agent session recordings.

pub mod backup;
pub mod filename;
pub mod lock;
pub mod resolve;
pub mod template;

/// Extract the filename from a path as a string.
pub(crate) fn filename_to_string(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Build a hidden auxiliary path by prepending a dot to the filename and appending a suffix.
///
/// Given `dir/session.cast` and suffix `.lock`, produces `dir/.session.cast.lock`.
/// If the filename already starts with a dot, no extra dot is added.
pub(crate) fn hidden_auxiliary_path(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new(""));
    let filename = filename_to_string(path);
    let hidden_name = if filename.starts_with('.') {
        format!("{}{}", filename, suffix)
    } else {
        format!(".{}{}", filename, suffix)
    };
    if parent.as_os_str().is_empty() {
        std::path::PathBuf::from(hidden_name)
    } else {
        parent.join(hidden_name)
    }
}

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
