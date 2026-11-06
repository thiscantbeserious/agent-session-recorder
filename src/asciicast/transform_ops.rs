//! High-level transform operations for asciicast files.
//!
//! This module provides file-level transform functions including backup management,
//! transform application, and restore operations. These are higher-level operations
//! built on top of the core transform traits.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

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

/// Apply all transforms to a recording file, creating a `.bak` if none exists.
///
/// This is the TUI path. See [`optimize_file`] for the shared steps.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed, the header threshold is not
/// positive and finite, backup creation fails, or writing the transformed file fails.
pub fn apply_transforms(path: &Path) -> Result<TransformResult> {
    optimize_file(path, true)
}

/// Remove silence from a recording without creating or reading any backup.
///
/// Works on `.cast` and `.cast.gz` (format is kept). Used by auto-optimize.
///
/// # Errors
///
/// Same as [`apply_transforms`], minus backup creation.
pub fn remove_silence(path: &Path) -> Result<TransformResult> {
    optimize_file(path, false)
}

// ponytail: whole file parsed in memory (~2-3x plain size), streaming optimize is the
// upgrade path (follow-up issue, ADR section 5).
fn optimize_file(path: &Path, backup: bool) -> Result<TransformResult> {
    let mut cast = AsciicastFile::parse(path)
        .with_context(|| format!("Failed to parse asciicast file: {}", path.display()))?;
    let original_duration = cast.duration();

    let threshold = cast
        .header
        .idle_time_limit
        .unwrap_or(DEFAULT_SILENCE_THRESHOLD);
    if !(threshold > 0.0 && threshold.is_finite()) {
        bail!("Invalid idle_time_limit in header: {threshold}");
    }

    let (backup_path, backup_created) = if backup {
        (Some(backup_path_for(path)), create_backup(path)?)
    } else {
        (None, false)
    };

    SilenceRemoval::new(threshold).transform(&mut cast.events);
    let new_duration = cast.duration();

    // AsciicastFile::write uses atomic temp+rename
    cast.write(path)
        .with_context(|| format!("Failed to write transformed file: {}", path.display()))?;

    Ok(TransformResult {
        original_duration,
        new_duration,
        backup_path,
        backup_created,
    })
}
