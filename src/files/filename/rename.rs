//! Renaming a session file together with its hidden backup.

use super::{is_valid_filename_char, MAX_FILENAME_LENGTH, WINDOWS_RESERVED};
use crate::files::backup::backup_path_for;
use crate::files::codec;
use std::ffi::OsStr;

/// Errors that can occur during file rename.
#[derive(Debug)]
pub enum RenameError {
    /// New name is empty.
    EmptyName,
    /// New name contains invalid characters.
    InvalidChars,
    /// New name is a Windows reserved name.
    ReservedName,
    /// New filename exceeds filesystem length limit.
    TooLong,
    /// A file with the new name already exists.
    AlreadyExists(String),
    /// Filesystem I/O error.
    IoError(std::io::Error),
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenameError::EmptyName => write!(f, "Name cannot be empty"),
            RenameError::InvalidChars => write!(f, "Name contains invalid characters"),
            RenameError::ReservedName => write!(f, "Name is a reserved system name"),
            RenameError::TooLong => write!(f, "Name is too long"),
            RenameError::AlreadyExists(name) => write!(f, "File already exists: {}", name),
            RenameError::IoError(e) => write!(f, "Rename failed: {}", e),
        }
    }
}

impl std::error::Error for RenameError {}

/// Rename a file on disk, preserving its original extension.
///
/// Validates the new stem, builds the new path in the same directory with
/// the original extension, and performs the filesystem rename. If a backup
/// file exists (via `backup_path_for`), it is renamed too (best-effort).
///
/// Returns the new path on success, or the original path if the name is unchanged.
pub fn rename_file(
    old_path: &std::path::Path,
    new_stem: &str,
) -> Result<std::path::PathBuf, RenameError> {
    validate_new_stem(new_stem)?;
    let (current_stem, suffix) = split_name(old_path);
    if new_stem == current_stem {
        return Ok(old_path.to_path_buf());
    }
    let new_filename = format!("{new_stem}{suffix}");
    if new_filename.len() > MAX_FILENAME_LENGTH {
        return Err(RenameError::TooLong);
    }
    let new_path = old_path.with_file_name(&new_filename);
    if new_path.exists() {
        return Err(RenameError::AlreadyExists(new_filename));
    }
    std::fs::rename(old_path, &new_path).map_err(RenameError::IoError)?;
    move_backup(old_path, &new_path);
    Ok(new_path)
}

/// Stem and suffix of a file name, `.cast.gz` and `.cast` whole, any other extension as is.
fn split_name(path: &std::path::Path) -> (&str, String) {
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or("");
    let stem = codec::session_stem(name);
    if stem != name {
        return (stem, codec::session_suffix(path).into());
    }
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("");
    let ext = path.extension().and_then(OsStr::to_str);
    (stem, ext.map(|e| format!(".{e}")).unwrap_or_default())
}

fn validate_new_stem(new_stem: &str) -> Result<(), RenameError> {
    if new_stem.trim().is_empty() {
        return Err(RenameError::EmptyName);
    }
    if new_stem.chars().any(|c| !is_valid_filename_char(c)) {
        return Err(RenameError::InvalidChars);
    }
    let base_name = new_stem.split('.').next().unwrap_or(new_stem);
    let upper = base_name.to_uppercase();
    if WINDOWS_RESERVED.iter().any(|r| upper == *r) {
        return Err(RenameError::ReservedName);
    }
    Ok(())
}

/// Best-effort move of the hidden backup (or a legacy visible one) next to the renamed file.
fn move_backup(old_path: &std::path::Path, new_path: &std::path::Path) {
    let hidden = backup_path_for(old_path);
    let source = if hidden.exists() {
        hidden
    } else {
        crate::files::backup::old_backup_path_for(old_path)
    };
    if source.exists() {
        let _ = std::fs::rename(&source, backup_path_for(new_path));
    }
}
