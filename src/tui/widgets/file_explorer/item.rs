//! `FileItem`: one session file shown in the explorer.

use chrono::{DateTime, Local};

use crate::files::backup::has_backup;
use crate::files::lock::{self, LockInfo};
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
    /// Lock info if file is actively being recorded (cached)
    pub lock_info: Option<LockInfo>,
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
        let lock_info = lock::read_lock(std::path::Path::new(&path_str));
        Self {
            path: path_str,
            name: name.into(),
            agent: agent.into(),
            size,
            modified,
            has_backup,
            lock_info,
        }
    }
}

impl From<SessionInfo> for FileItem {
    fn from(session: SessionInfo) -> Self {
        let path_str = session.path.to_string_lossy().to_string();
        let has_backup = has_backup(std::path::Path::new(&path_str));
        let lock_info = lock::read_lock(std::path::Path::new(&path_str));
        Self {
            path: path_str,
            name: session.filename,
            agent: session.agent,
            size: session.size,
            modified: session.modified,
            has_backup,
            lock_info,
        }
    }
}
