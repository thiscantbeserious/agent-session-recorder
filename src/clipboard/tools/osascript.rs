//! macOS AppleScript clipboard tool.

use crate::clipboard::result::CopyMethod;
use crate::clipboard::tool::{CopyTool, CopyToolError};
use std::path::Path;
use std::process::Command;

/// macOS AppleScript clipboard tool.
///
/// Uses `osascript` to copy files as POSIX file references.
/// This allows pasting as actual file attachments in Slack, etc.
pub struct OsaScript;

impl OsaScript {
    /// Create a new OsaScript tool.
    pub fn new() -> Self {
        Self
    }

    /// Escape a path for use in AppleScript string.
    ///
    /// Escapes backslashes, double quotes, and control characters
    /// (newlines, carriage returns, tabs) to prevent AppleScript injection
    /// or syntax errors.
    pub fn escape_path(path: &Path) -> String {
        path.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    }

    /// Build the AppleScript command for file copy.
    ///
    /// Uses `set the clipboard to POSIX file` which sets all required
    /// pasteboard types (public.file-url, NSFilenamesPboardType, etc.)
    /// that Finder, Slack, and other apps expect for paste operations.
    pub fn build_file_script(path: &Path) -> String {
        format!(
            "set the clipboard to POSIX file \"{}\"",
            Self::escape_path(path)
        )
    }

    /// Run an AppleScript.
    fn run_script(script: &str) -> Result<(), CopyToolError> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| CopyToolError::Failed(e.to_string()))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(CopyToolError::Failed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}

impl CopyTool for OsaScript {
    fn method(&self) -> CopyMethod {
        CopyMethod::OsaScript
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn can_copy_files(&self) -> bool {
        true
    }

    fn try_copy_file(&self, path: &Path) -> Result<(), CopyToolError> {
        let script = Self::build_file_script(path);
        Self::run_script(&script)
    }

    fn try_copy_text(&self, _text: &str) -> Result<(), CopyToolError> {
        // osascript can do text, but pbcopy is simpler/faster
        Err(CopyToolError::NotSupported)
    }
}

impl Default for OsaScript {
    fn default() -> Self {
        Self::new()
    }
}
