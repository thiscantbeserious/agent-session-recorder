//! Session listing and lookup

use super::StorageManager;
use anyhow::Result;
use chrono::{DateTime, Datelike, Local};
use humansize::{format_size, BINARY};
use std::fs;
use std::path::PathBuf;

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

impl SessionInfo {
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
    /// List all sessions, optionally filtered by agent
    pub fn list_sessions(&self, agent: Option<&str>) -> Result<Vec<SessionInfo>> {
        let storage_dir = self.storage_dir();
        if !storage_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        let now = Local::now();

        // If agent specified, only check that directory
        let agent_dirs: Vec<PathBuf> = if let Some(agent_name) = agent {
            let agent_dir = storage_dir.join(agent_name);
            if agent_dir.exists() {
                vec![agent_dir]
            } else {
                vec![]
            }
        } else {
            // Check all subdirectories
            fs::read_dir(&storage_dir)?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        };

        for agent_dir in agent_dirs {
            let agent_name = agent_dir
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("unknown")
                .to_string();

            for entry in fs::read_dir(&agent_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().is_some_and(|ext| ext == "cast") {
                    let metadata = fs::metadata(&path)?;
                    let modified: DateTime<Local> = metadata.modified()?.into();
                    let duration = now - modified;
                    let age_days = duration.num_days();
                    let age_hours = duration.num_hours();
                    let age_minutes = duration.num_minutes();

                    sessions.push(SessionInfo {
                        filename: path
                            .file_name()
                            .and_then(std::ffi::OsStr::to_str)
                            .unwrap_or("")
                            .to_string(),
                        agent: agent_name.clone(),
                        size: metadata.len(),
                        modified,
                        age_days,
                        age_hours,
                        age_minutes,
                        path,
                    });
                }
            }
        }

        // Sort by modification time, newest first
        sessions.sort_by_key(|s| std::cmp::Reverse(s.modified));

        Ok(sessions)
    }

    /// Resolve a short path like "agent/file.cast" to a full path
    ///
    /// Returns None if the file doesn't exist at the resolved path.
    /// If the input is already an absolute path or exists as-is, returns it directly.
    pub fn resolve_cast_path(&self, path: &str) -> Option<PathBuf> {
        let path_buf = PathBuf::from(path);

        // If absolute path, return as-is if it exists
        if path_buf.is_absolute() {
            return if path_buf.exists() {
                Some(path_buf)
            } else {
                None
            };
        }

        // If it exists as a relative path from current directory, return it
        if path_buf.exists() {
            return Some(path_buf);
        }

        // Try to resolve as "agent/file.cast" format
        // Split on the first '/' to get agent and filename
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() == 2 {
            let agent = parts[0];
            let filename = parts[1];
            let full_path = self.storage_dir().join(agent).join(filename);
            if full_path.exists() {
                return Some(full_path);
            }
        }

        None
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
        let mut matches: Vec<_> = sessions.iter().filter(|s| s.filename == filename).collect();

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
