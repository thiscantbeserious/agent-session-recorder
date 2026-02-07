//! File item data model for the explorer widget.
//!
//! A standalone value type representing a session recording file.
//! Used across the entire codebase for listing and displaying sessions.

use chrono::{DateTime, Local};

use crate::files::backup::has_backup;
use crate::storage::SessionInfo;

/// A file item in the explorer
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileItem {
    /// Full path to the file
    pub path: String,
    /// Display name (filename without path)
    pub name: String,
    /// Agent name (e.g., "claude", "codex")
    pub agent: String,
    /// File size in bytes
    pub size: u64,
    /// Last modified time
    pub modified: DateTime<Local>,
    /// Whether a backup file exists for this item (cached)
    pub has_backup: bool,
}

impl FileItem {
    /// Create a new FileItem
    pub fn new(
        path: impl Into<String>,
        name: impl Into<String>,
        agent: impl Into<String>,
        size: u64,
        modified: DateTime<Local>,
    ) -> Self {
        let path_str = path.into();
        let has_backup = has_backup(std::path::Path::new(&path_str));
        Self {
            path: path_str,
            name: name.into(),
            agent: agent.into(),
            size,
            modified,
            has_backup,
        }
    }
}

impl From<SessionInfo> for FileItem {
    fn from(session: SessionInfo) -> Self {
        let path_str = session.path.to_string_lossy().to_string();
        let has_backup = has_backup(std::path::Path::new(&path_str));
        Self {
            path: path_str,
            name: session.filename,
            agent: session.agent,
            size: session.size,
            modified: session.modified,
            has_backup,
        }
    }
}

/// Format a byte size as human-readable string
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

    #[test]
    fn format_size_works() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
    }

    #[test]
    fn file_item_from_session_info() {
        let session = SessionInfo {
            path: PathBuf::from("/sessions/claude/test.cast"),
            agent: "claude".to_string(),
            filename: "test.cast".to_string(),
            size: 1024,
            modified: Local.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            age_days: 0,
            age_hours: 0,
            age_minutes: 0,
        };

        let item = FileItem::from(session);
        assert_eq!(item.path, "/sessions/claude/test.cast");
        assert_eq!(item.name, "test.cast");
        assert_eq!(item.agent, "claude");
        assert_eq!(item.size, 1024);
    }
}
