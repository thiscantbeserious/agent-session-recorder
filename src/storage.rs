//! Storage management for recorded sessions

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use humansize::{format_size, BINARY};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;

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

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_size: u64,
    pub session_count: usize,
    pub sessions_by_agent: HashMap<String, usize>,
    pub oldest_session: Option<SessionInfo>,
    pub disk_percentage: f64,
}

impl StorageStats {
    /// Get human-readable total size
    pub fn size_human(&self) -> String {
        format_size(self.total_size, BINARY)
    }

    /// Format a summary for display
    pub fn summary(&self) -> String {
        // Sort agents alphabetically for consistent output
        let mut agents: Vec<_> = self.sessions_by_agent.iter().collect();
        agents.sort_by(|a, b| a.0.cmp(b.0));

        let agents_summary: Vec<String> = agents
            .iter()
            .map(|(agent, count)| format!("{}: {}", agent, count))
            .collect();

        let agents_display = if agents_summary.is_empty() {
            String::new()
        } else {
            format!(" ({})", agents_summary.join(", "))
        };

        let mut summary = format!(
            "Agent Sessions: {} ({:.2}% of disk)\n   Sessions: {} total{}",
            self.size_human(),
            self.disk_percentage,
            self.session_count,
            agents_display
        );

        if let Some(oldest) = &self.oldest_session {
            summary.push_str(&format!(
                "\n   Oldest: {} ({} days ago)",
                oldest.modified.format("%Y-%m-%d"),
                oldest.age_days
            ));
        }

        summary
    }
}

/// Storage manager for session recordings
pub struct StorageManager {
    config: Config,
}

impl StorageManager {
    /// Create a new storage manager with the given config
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Get the storage directory path
    pub fn storage_dir(&self) -> PathBuf {
        self.config.storage_directory()
    }

    /// Ensure the storage directory exists
    pub fn ensure_storage_dir(&self) -> Result<PathBuf> {
        let dir = self.storage_dir();
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create storage directory: {:?}", dir))?;
        }
        Ok(dir)
    }

    /// Ensure an agent's session directory exists
    pub fn ensure_agent_dir(&self, agent: &str) -> Result<PathBuf> {
        let dir = self.storage_dir().join(agent);
        if !dir.exists() {
            fs::create_dir_all(&dir)
                .with_context(|| format!("Failed to create agent directory: {:?}", dir))?;
        }
        Ok(dir)
    }

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
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect()
        };

        for agent_dir in agent_dirs {
            let agent_name = agent_dir
                .file_name()
                .and_then(|n| n.to_str())
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
                            .and_then(|n| n.to_str())
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

        // Sort by modification time (oldest first)
        sessions.sort_by(|a, b| a.modified.cmp(&b.modified));

        Ok(sessions)
    }

    /// Get storage statistics
    pub fn get_stats(&self) -> Result<StorageStats> {
        let sessions = self.list_sessions(None)?;

        let total_size: u64 = sessions.iter().map(|s| s.size).sum();
        let session_count = sessions.len();

        let mut sessions_by_agent: HashMap<String, usize> = HashMap::new();
        for session in &sessions {
            *sessions_by_agent.entry(session.agent.clone()).or_insert(0) += 1;
        }

        let oldest_session = sessions.first().cloned();

        // Calculate disk percentage (simplified - uses available space)
        let disk_percentage = self.calculate_disk_percentage(total_size);

        Ok(StorageStats {
            total_size,
            session_count,
            sessions_by_agent,
            oldest_session,
            disk_percentage,
        })
    }

    /// Calculate what percentage of disk the storage uses
    fn calculate_disk_percentage(&self, total_size: u64) -> f64 {
        // Get total disk size using df command (works on macOS and Linux)
        let storage_dir = self.storage_dir();
        let path_str = storage_dir.to_string_lossy();

        // Try to get disk info using df command
        if let Ok(output) = std::process::Command::new("df")
            .arg("-k") // Use 1K blocks for consistent parsing
            .arg(&*path_str)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // Parse df output - second line contains the data
                // Format: Filesystem 1K-blocks Used Available Use% Mounted
                if let Some(line) = stdout.lines().nth(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    // parts[1] is total blocks in KB
                    if parts.len() >= 2 {
                        if let Ok(total_kb) = parts[1].parse::<u64>() {
                            let total_bytes = total_kb * 1024;
                            if total_bytes > 0 {
                                return (total_size as f64 / total_bytes as f64) * 100.0;
                            }
                        }
                    }
                }
            }
        }

        // Fallback: return 0.0 if we can't determine disk size
        0.0
    }

    /// Delete sessions by path
    pub fn delete_sessions(&self, sessions: &[SessionInfo]) -> Result<u64> {
        let mut freed_size = 0u64;

        for session in sessions {
            if session.path.exists() {
                fs::remove_file(&session.path)
                    .with_context(|| format!("Failed to delete: {:?}", session.path))?;
                freed_size += session.size;
            }
        }

        Ok(freed_size)
    }

    /// Check if storage exceeds threshold
    pub fn exceeds_threshold(&self) -> Result<bool> {
        let stats = self.get_stats()?;
        let threshold_bytes =
            (self.config.storage.size_threshold_gb * 1024.0 * 1024.0 * 1024.0) as u64;
        Ok(stats.total_size > threshold_bytes)
    }

    /// Get sessions older than the configured age threshold
    pub fn get_old_sessions(&self) -> Result<Vec<SessionInfo>> {
        let sessions = self.list_sessions(None)?;
        let threshold_days = self.config.storage.age_threshold_days as i64;

        Ok(sessions
            .into_iter()
            .filter(|s| s.age_days > threshold_days)
            .collect())
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
        matches.sort_by(|a, b| b.modified.cmp(&a.modified));
        matches.first().map(|s| s.path.clone())
    }

    /// List all cast files in short format (agent/filename.cast)
    ///
    /// Optionally filter by a prefix (e.g., "claude/" to list only claude sessions)
    pub fn list_cast_files_short(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let sessions = self.list_sessions(None)?;

        let mut files: Vec<String> = sessions
            .iter()
            .map(|s| format!("{}/{}", s.agent, s.filename))
            .collect();

        // Filter by prefix if provided
        if let Some(prefix) = prefix {
            files.retain(|f| f.starts_with(prefix));
        }

        // Sort alphabetically
        files.sort();

        Ok(files)
    }
}
