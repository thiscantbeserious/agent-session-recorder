//! Copy orchestrator for clipboard operations.

use super::error::{ClipboardError, MAX_CONTENT_SIZE};
use super::result::CopyResult;
use super::tool::{CopyTool, CopyToolError};
use super::tools::platform_tools;
use std::path::Path;

/// Orchestrates clipboard copy operations using available tools.
///
/// Tries tools in priority order:
/// 1. File copy with tools that support it
/// 2. Content copy as fallback (with size limit)
pub struct Copy {
    tools: Vec<Box<dyn CopyTool>>,
}

impl Copy {
    /// Create with platform-appropriate tools.
    pub fn new() -> Self {
        Self {
            tools: platform_tools(),
        }
    }

    /// Create with specific tools (for testing).
    pub fn with_tools(tools: Vec<Box<dyn CopyTool>>) -> Self {
        Self { tools }
    }

    /// Get a reference to the tools list.
    pub fn tools(&self) -> &[Box<dyn CopyTool>] {
        &self.tools
    }

    /// Copy a file to the clipboard.
    ///
    /// Tries file copy first, falls back to content copy.
    /// Content fallback has a size limit to prevent memory exhaustion.
    pub fn file(&self, path: &Path) -> Result<CopyResult, ClipboardError> {
        // Validate file exists
        if !path.exists() {
            return Err(ClipboardError::FileNotFound {
                path: path.to_path_buf(),
            });
        }

        // Try file copy with tools that support it
        let mut last_error: Option<String> = None;
        for tool in &self.tools {
            if tool.is_available() && tool.can_copy_files() {
                match tool.try_copy_file(path) {
                    Ok(()) => {
                        return Ok(CopyResult::file_copied(tool.method()));
                    }
                    Err(CopyToolError::NotSupported) => continue,
                    Err(CopyToolError::NotFound) => continue,
                    Err(CopyToolError::Failed(msg)) => {
                        // Log the error for debugging, then try next tool
                        eprintln!(
                            "Clipboard: {} failed ({}), trying next tool...",
                            tool.name(),
                            msg
                        );
                        last_error = Some(msg);
                        continue;
                    }
                }
            }
        }

        // Check file size before content fallback to prevent memory exhaustion
        let metadata = std::fs::metadata(path)?;
        if metadata.len() > MAX_CONTENT_SIZE {
            return Err(ClipboardError::FileTooLarge {
                size_mb: metadata.len() as f64 / (1024.0 * 1024.0),
                max_mb: MAX_CONTENT_SIZE / (1024 * 1024),
            });
        }

        // Fall back to content copy
        let content = std::fs::read_to_string(path)?;
        let size = content.len();

        for tool in &self.tools {
            if tool.is_available() {
                match tool.try_copy_text(&content) {
                    Ok(()) => {
                        return Ok(CopyResult::content_copied(tool.method(), size));
                    }
                    Err(CopyToolError::NotSupported) => continue,
                    Err(CopyToolError::NotFound) => continue,
                    Err(CopyToolError::Failed(msg)) => {
                        eprintln!(
                            "Clipboard: {} text copy failed ({}), trying next tool...",
                            tool.name(),
                            msg
                        );
                        last_error = Some(msg);
                        continue;
                    }
                }
            }
        }

        // Include last error in debug output if all tools failed
        if let Some(err) = last_error {
            eprintln!("Clipboard: All tools failed. Last error: {}", err);
        }

        Err(ClipboardError::NoToolAvailable)
    }
}

impl Default for Copy {
    fn default() -> Self {
        Self::new()
    }
}
