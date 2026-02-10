//! Copy operation results and method identifiers.

/// The result of a clipboard copy operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    /// File was copied as a file reference (can paste as file attachment)
    FileCopied { tool: CopyMethod },
    /// File content was copied as text (fallback when file copy unavailable)
    ContentCopied { tool: CopyMethod, size_bytes: usize },
}

impl CopyResult {
    /// Create a FileCopied result.
    pub fn file_copied(tool: CopyMethod) -> Self {
        Self::FileCopied { tool }
    }

    /// Create a ContentCopied result.
    pub fn content_copied(tool: CopyMethod, size_bytes: usize) -> Self {
        Self::ContentCopied { tool, size_bytes }
    }

    /// User-friendly message describing what happened.
    pub fn message(&self, filename: &str) -> String {
        match self {
            Self::FileCopied { .. } => {
                format!("🎬 Copied {}.cast to clipboard", filename)
            }
            Self::ContentCopied { .. } => {
                format!(
                    "🎬 Copied {}.cast content to clipboard (file copy not supported on this platform)",
                    filename
                )
            }
        }
    }

    /// Whether this was a true file copy (not content fallback).
    pub fn is_file_copy(&self) -> bool {
        matches!(self, Self::FileCopied { .. })
    }
}

/// Which tool was used for the copy operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMethod {
    /// macOS AppleScript
    OsaScript,
    /// macOS pasteboard
    Pbcopy,
    /// Linux X11
    Xclip,
    /// Linux X11 alternative
    Xsel,
    /// Linux Wayland
    WlCopy,
}

impl CopyMethod {
    /// Tool name for display/logging.
    pub fn name(&self) -> &'static str {
        match self {
            Self::OsaScript => "osascript",
            Self::Pbcopy => "pbcopy",
            Self::Xclip => "xclip",
            Self::Xsel => "xsel",
            Self::WlCopy => "wl-copy",
        }
    }
}
