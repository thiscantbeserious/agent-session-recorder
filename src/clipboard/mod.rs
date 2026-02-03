//! Clipboard operations for copying recordings.
//!
//! This module provides cross-platform clipboard support for copying
//! `.cast` recording files. On macOS, files can be copied as file references
//! for direct paste into Slack/email. On Linux, it falls back to content copy
//! when file copy isn't supported.
//!
//! # Example
//!
//! ```ignore
//! use agr::clipboard::copy_file_to_clipboard;
//! use std::path::Path;
//!
//! let result = copy_file_to_clipboard(Path::new("/path/to/recording.cast"))?;
//! println!("{}", result.message("recording"));
//! ```

pub mod copy;
mod error;
mod result;
pub mod tool;
pub mod tools;

pub use error::ClipboardError;
pub use result::{CopyMethod, CopyResult};

use copy::Copy;
use std::path::Path;

/// Copy a file to the system clipboard.
///
/// Tries to copy the file as a file reference first (for paste-as-file in Slack, etc.).
/// Falls back to copying the file's text content if file copy isn't supported.
///
/// # Errors
/// - `ClipboardError::FileNotFound` - file doesn't exist
/// - `ClipboardError::NoToolAvailable` - no clipboard tool found
/// - `ClipboardError::FileTooLarge` - file exceeds size limit for content fallback
pub fn copy_file_to_clipboard(path: &Path) -> Result<CopyResult, ClipboardError> {
    Copy::new().file(path)
}
