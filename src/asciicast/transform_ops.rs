//! High-level transform operations for asciicast files.
//!
//! This module provides file-level transform functions including backup management,
//! transform application, and restore operations. These are higher-level operations
//! built on top of the core transform traits.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::{AsciicastFile, SilenceRemoval, Transform, DEFAULT_SILENCE_THRESHOLD};
use crate::files::backup::{backup_path_for, create_backup};

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
    let backup_created = create_backup(path)?;
    let backup = backup_path_for(path);

    // Resolve threshold: header's idle_time_limit or default
    let threshold = cast
        .header
        .idle_time_limit
        .unwrap_or(DEFAULT_SILENCE_THRESHOLD);

    // Apply silence removal transform
    let mut transform = SilenceRemoval::new(threshold);
    transform.transform(&mut cast.events);

    let new_duration = cast.duration();

    // Write back to original path (AsciicastFile::write uses atomic temp+rename)
    cast.write(path)
        .with_context(|| format!("Failed to write transformed file: {}", path.display()))?;

    Ok(TransformResult {
        original_duration,
        new_duration,
        backup_path: Some(backup),
        backup_created,
    })
}
