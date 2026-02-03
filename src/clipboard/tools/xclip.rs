//! Linux xclip clipboard tool.

use crate::clipboard::result::CopyMethod;
use crate::clipboard::tool::{CopyTool, CopyToolError};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Linux X11 clipboard tool using xclip.
///
/// Uses `xclip` to copy files as file URIs or text content.
pub struct Xclip;

impl Xclip {
    /// Create a new Xclip tool.
    pub fn new() -> Self {
        Self
    }

    /// Build a file:// URI for the given path.
    ///
    /// Spaces and special characters are percent-encoded per RFC 3986.
    /// Non-ASCII characters are encoded as UTF-8 bytes.
    pub fn build_file_uri(path: &Path) -> String {
        let path_str = path.display().to_string();
        let mut encoded = String::new();

        for c in path_str.chars() {
            if c.is_ascii_alphanumeric() || c == '/' || c == '.' || c == '-' || c == '_' {
                encoded.push(c);
            } else {
                // Encode as UTF-8 bytes per RFC 3986
                for byte in c.to_string().as_bytes() {
                    encoded.push_str(&format!("%{:02X}", byte));
                }
            }
        }

        format!("file://{}", encoded)
    }

    /// Check if xclip is installed.
    fn tool_exists() -> bool {
        Command::new("which")
            .arg("xclip")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

impl CopyTool for Xclip {
    fn method(&self) -> CopyMethod {
        CopyMethod::Xclip
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "linux") && Self::tool_exists()
    }

    fn can_copy_files(&self) -> bool {
        true
    }

    fn try_copy_file(&self, path: &Path) -> Result<(), CopyToolError> {
        let uri = Self::build_file_uri(path);

        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "text/uri-list"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| CopyToolError::Failed(e.to_string()))?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(uri.as_bytes())
                .map_err(|e| CopyToolError::Failed(e.to_string()))?;
        }

        let status = child
            .wait()
            .map_err(|e| CopyToolError::Failed(e.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(CopyToolError::Failed("xclip failed".to_string()))
        }
    }

    fn try_copy_text(&self, text: &str) -> Result<(), CopyToolError> {
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| CopyToolError::Failed(e.to_string()))?;

        if let Some(stdin) = child.stdin.as_mut() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| CopyToolError::Failed(e.to_string()))?;
        }

        let status = child
            .wait()
            .map_err(|e| CopyToolError::Failed(e.to_string()))?;

        if status.success() {
            Ok(())
        } else {
            Err(CopyToolError::Failed("xclip failed".to_string()))
        }
    }
}

impl Default for Xclip {
    fn default() -> Self {
        Self::new()
    }
}
