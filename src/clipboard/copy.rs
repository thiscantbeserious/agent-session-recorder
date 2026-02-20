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
        if !path.exists() {
            return Err(ClipboardError::FileNotFound {
                path: path.to_path_buf(),
            });
        }

        if let Ok(result) = self.try_copy_file_with_tools(path) {
            return Ok(result);
        }

        let metadata = std::fs::metadata(path)?;
        if metadata.len() > MAX_CONTENT_SIZE {
            return Err(ClipboardError::FileTooLarge {
                size_mb: metadata.len() as f64 / (1024.0 * 1024.0),
                max_mb: MAX_CONTENT_SIZE / (1024 * 1024),
            });
        }

        let content = std::fs::read_to_string(path)?;

        match self.try_copy_text_with_tools(&content) {
            Ok(result) => Ok(result),
            Err(last_error) => {
                if let Some(err) = last_error {
                    eprintln!("Clipboard: All tools failed. Last error: {}", err);
                }
                Err(ClipboardError::NoToolAvailable)
            }
        }
    }

    /// Iterate available file-capable tools and attempt file copy.
    ///
    /// Returns `Ok(CopyResult)` on first success. Returns `Err(Some(msg))` if
    /// a tool reported a failure, or `Err(None)` if no eligible tool was found.
    fn try_copy_file_with_tools(&self, path: &Path) -> Result<CopyResult, Option<String>> {
        let mut last_error: Option<String> = None;
        for tool in &self.tools {
            if !tool.is_available() || !tool.can_copy_files() {
                continue;
            }
            match tool.try_copy_file(path) {
                Ok(()) => return Ok(CopyResult::file_copied(tool.method())),
                Err(CopyToolError::NotSupported) | Err(CopyToolError::NotFound) => continue,
                Err(CopyToolError::Failed(msg)) => {
                    eprintln!(
                        "Clipboard: {} failed ({}), trying next tool...",
                        tool.name(),
                        msg
                    );
                    last_error = Some(msg);
                }
            }
        }
        Err(last_error)
    }

    /// Iterate available tools and attempt text copy.
    ///
    /// Returns `Ok(CopyResult)` on first success. Returns `Err(Some(msg))` if
    /// a tool reported a failure, or `Err(None)` if no eligible tool was found.
    fn try_copy_text_with_tools(&self, content: &str) -> Result<CopyResult, Option<String>> {
        let size = content.len();
        let mut last_error: Option<String> = None;
        for tool in &self.tools {
            if !tool.is_available() {
                continue;
            }
            match tool.try_copy_text(content) {
                Ok(()) => return Ok(CopyResult::content_copied(tool.method(), size)),
                Err(CopyToolError::NotSupported) | Err(CopyToolError::NotFound) => continue,
                Err(CopyToolError::Failed(msg)) => {
                    eprintln!(
                        "Clipboard: {} text copy failed ({}), trying next tool...",
                        tool.name(),
                        msg
                    );
                    last_error = Some(msg);
                }
            }
        }
        Err(last_error)
    }
}

impl Default for Copy {
    fn default() -> Self {
        Self::new()
    }
}
