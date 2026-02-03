//! Clipboard operation errors.

use std::path::PathBuf;

/// Maximum file size for content fallback (10 MB).
pub const MAX_CONTENT_SIZE: u64 = 10 * 1024 * 1024;

/// Errors that can occur during clipboard operations.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("No clipboard tool available. On Linux, install xclip, xsel, or wl-copy.")]
    NoToolAvailable,

    #[error("File too large for clipboard ({size_mb:.1} MB). Maximum is {max_mb} MB.")]
    FileTooLarge { size_mb: f64, max_mb: u64 },

    #[error("Failed to read file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Platform not supported (only macOS and Linux)")]
    UnsupportedPlatform,
}
