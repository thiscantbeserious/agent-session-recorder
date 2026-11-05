//! Session listing and lookup

use super::StorageManager;
use crate::files::codec;
use anyhow::Result;
use chrono::{DateTime, Datelike, Local};
use humansize::{format_size, BINARY};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Format a timestamp as a smart time prefix:
/// - Today:      `14:05` (H:MM)
/// - Yesterday:  `23h`   (hours ago)
/// - Same year:  `01.02` (DD.MM)
/// - Older:      `1y5m`  (years and months ago)
pub fn format_smart_time(modified: &DateTime<Local>) -> String {
    let now = Local::now();
    let duration = now.signed_duration_since(*modified);
    let hours = duration.num_hours();

    if duration.num_seconds() < 0 {
        // Future timestamp — just show time
        return modified.format("%H:%M").to_string();
    }

    if hours < 24 && now.date_naive() == modified.date_naive() {
        // Today — show H:MM
        modified.format("%H:%M").to_string()
    } else if hours < 48 {
        // Yesterday — show hours
        format!("{}h", hours)
    } else if modified.year() == now.year() {
        // Same year — show DD.MM
        modified.format("%d.%m").to_string()
    } else {
        // Older — show relative years and months
        let mut years = now.year() - modified.year();
        let mut months = now.month() as i32 - modified.month() as i32;
        if months < 0 {
            years -= 1;
            months += 12;
        }
        if years > 0 {
            format!("{}y{}m", years, months)
        } else {
            format!("{}m", months)
        }
    }
}

/// The path itself when it exists, else its other variant when that exists.
fn existing_variant(path: &Path) -> Option<PathBuf> {
    [path.to_path_buf(), codec::other_variant(path)]
        .into_iter()
        .find(|p| p.exists())
}

/// Agent directories to scan, one when filtered, else all subdirectories.
fn agent_dirs(storage_dir: &Path, agent: Option<&str>) -> Result<Vec<PathBuf>> {
    if let Some(name) = agent {
        let dir = storage_dir.join(name);
        return Ok(if dir.exists() { vec![dir] } else { vec![] });
    }
    Ok(fs::read_dir(storage_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect())
}

/// Sessions of one agent directory, hiding a `.cast` that has a `.cast.gz` sibling.
fn agent_sessions(agent_dir: &Path) -> Result<Vec<SessionInfo>> {
    let agent = agent_dir
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("unknown");
    let mut sessions = Vec::new();
    for entry in fs::read_dir(agent_dir)? {
        let path = entry?.path();
        if codec::is_session_path(&path)
            && (codec::is_gzip_path(&path) || !codec::other_variant(&path).exists())
        {
            sessions.push(session_info(path, agent)?);
        }
    }
    Ok(sessions)
}

fn session_info(path: PathBuf, agent: &str) -> Result<SessionInfo> {
    let metadata = fs::metadata(&path)?;
    let modified: DateTime<Local> = metadata.modified()?.into();
    let duration = Local::now() - modified;
    Ok(SessionInfo {
        filename: path
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("")
            .to_string(),
        agent: agent.to_string(),
        size: metadata.len(),
        modified,
        age_days: duration.num_days(),
        age_hours: duration.num_hours(),
        age_minutes: duration.num_minutes(),
        path,
    })
}

/// Information about a recorded session
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub path: PathBuf,
    pub agent: String,
    pub filename: String,
    pub size: u64,
    pub modified: DateTime<Local>,
    pub age_days: i64,
    pub age_hours: i64,
    pub age_minutes: i64,
}

/// Human-readable uncompressed size, "4GiB+" when only a wrapped size is known.
pub fn format_uncompressed(size: codec::UncompressedSize) -> String {
    match size {
        codec::UncompressedSize::Exact(n) => format_size(n, BINARY),
        codec::UncompressedSize::Over4GiB => "4GiB+".to_string(),
    }
}

impl SessionInfo {
    /// Uncompressed size read from the file header, computed on demand (never decompresses).
    pub fn uncompressed_size(&self) -> Result<codec::UncompressedSize> {
        codec::uncompressed_size(&self.path)
    }

    /// Get human-readable size
    pub fn size_human(&self) -> String {
        format_size(self.size, BINARY)
    }

    /// Format age for display - smart format based on age
    /// - <1 hour: "  45m" (minutes only)
    /// - <1 day:  "   5h" (hours only)
    /// - >=1 day: "   3d" (days only)
    pub fn format_age(&self) -> String {
        if self.age_hours == 0 {
            // Less than 1 hour: show minutes
            format!("{:>4}m", self.age_minutes)
        } else if self.age_days == 0 {
            // Same day: show hours only
            format!("{:>4}h", self.age_hours)
        } else {
            // Older: show days only
            format!("{:>4}d", self.age_days)
        }
    }
}

impl StorageManager {
    /// List all sessions, optionally filtered by agent.
    ///
    /// `x.cast` and `x.cast.gz` are one session, only the `.cast.gz` is listed.
    pub fn list_sessions(&self, agent: Option<&str>) -> Result<Vec<SessionInfo>> {
        let storage_dir = self.storage_dir();
        if !storage_dir.exists() {
            return Ok(Vec::new());
        }
        let mut sessions = Vec::new();
        for agent_dir in agent_dirs(&storage_dir, agent)? {
            sessions.extend(agent_sessions(&agent_dir)?);
        }
        // Sort by modification time, newest first
        sessions.sort_by_key(|s| std::cmp::Reverse(s.modified));
        Ok(sessions)
    }

    /// Resolve a short path like "agent/file.cast" to a full path
    ///
    /// Returns None if neither the `.cast` nor the `.cast.gz` variant exists.
    /// If the input is already an absolute path or exists as-is, returns it directly.
    pub fn resolve_cast_path(&self, path: &str) -> Option<PathBuf> {
        let path_buf = PathBuf::from(path);
        if path_buf.is_absolute() || path_buf.exists() {
            return existing_variant(&path_buf);
        }
        // Try to resolve as "agent/file.cast" format
        let (agent, filename) = path.split_once('/')?;
        existing_variant(&self.storage_dir().join(agent).join(filename))
    }

    /// Find a cast file by filename only, searching across all agents
    ///
    /// Returns the first match found. If multiple agents have the same filename,
    /// returns the most recently modified one.
    ///
    /// # Arguments
    /// * `filename` - The filename to search for (e.g., "session.cast")
    ///
    /// # Returns
    /// * `Some(PathBuf)` - Full path to the found file
    /// * `None` - If no matching file is found
    pub fn find_cast_file_by_name(&self, filename: &str) -> Option<PathBuf> {
        let sessions = self.list_sessions(None).ok()?;

        // Find all sessions with matching filename
        if !codec::is_session_path(Path::new(filename)) {
            return None;
        }
        let stem = codec::session_stem(filename);
        let mut matches: Vec<_> = sessions
            .iter()
            .filter(|s| codec::session_stem(&s.filename) == stem)
            .collect();

        // Sort by modification time (newest first) and return the first match
        matches.sort_by_key(|s| std::cmp::Reverse(s.modified));
        matches.first().map(|s| s.path.clone())
    }

    /// List all cast files in short format (agent/filename.cast)
    ///
    /// Returns files sorted by modification time (most recent first).
    /// Optionally filter by a prefix (e.g., "claude/" to list only claude sessions)
    pub fn list_cast_files_short(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let mut sessions = self.list_sessions(None)?;

        // Sort by modification time (most recent first)
        sessions.sort_by_key(|s| std::cmp::Reverse(s.modified));

        let mut files: Vec<String> = sessions
            .iter()
            .map(|s| format!("{}/{}", s.agent, s.filename))
            .collect();

        // Filter by prefix if provided
        if let Some(prefix) = prefix {
            files.retain(|f| f.starts_with(prefix));
        }

        Ok(files)
    }
}
