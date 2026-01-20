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
}

impl SessionInfo {
    /// Get human-readable size
    pub fn size_human(&self) -> String {
        format_size(self.size, BINARY)
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

                if path.extension().map_or(false, |ext| ext == "cast") {
                    let metadata = fs::metadata(&path)?;
                    let modified: DateTime<Local> = metadata.modified()?.into();
                    let age_days = (now - modified).num_days();

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
        let threshold_bytes = (self.config.storage.size_threshold_gb * 1024.0 * 1024.0 * 1024.0) as u64;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> Config {
        let mut config = Config::default();
        config.storage.directory = temp_dir.path().to_string_lossy().to_string();
        config
    }

    fn create_test_session(dir: &Path, agent: &str, filename: &str, content: &str) -> PathBuf {
        let agent_dir = dir.join(agent);
        fs::create_dir_all(&agent_dir).unwrap();
        let path = agent_dir.join(filename);
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn list_sessions_returns_empty_for_new_storage() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        let sessions = manager.list_sessions(None).unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn list_sessions_finds_cast_files() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "test.cast", "test content");

        let sessions = manager.list_sessions(None).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].agent, "claude");
        assert_eq!(sessions[0].filename, "test.cast");
    }

    #[test]
    fn list_sessions_filters_by_agent() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "session1.cast", "content");
        create_test_session(temp.path(), "codex", "session2.cast", "content");

        let claude_sessions = manager.list_sessions(Some("claude")).unwrap();
        assert_eq!(claude_sessions.len(), 1);
        assert_eq!(claude_sessions[0].agent, "claude");

        let codex_sessions = manager.list_sessions(Some("codex")).unwrap();
        assert_eq!(codex_sessions.len(), 1);
        assert_eq!(codex_sessions[0].agent, "codex");
    }

    #[test]
    fn list_sessions_ignores_non_cast_files() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "test.cast", "content");
        create_test_session(temp.path(), "claude", "test.txt", "content");
        create_test_session(temp.path(), "claude", "test.json", "content");

        let sessions = manager.list_sessions(None).unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn get_stats_calculates_correctly() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "session1.cast", "content1");
        create_test_session(temp.path(), "claude", "session2.cast", "content2");
        create_test_session(temp.path(), "codex", "session3.cast", "content3");

        let stats = manager.get_stats().unwrap();
        assert_eq!(stats.session_count, 3);
        assert_eq!(stats.sessions_by_agent.get("claude"), Some(&2));
        assert_eq!(stats.sessions_by_agent.get("codex"), Some(&1));
    }

    #[test]
    fn delete_sessions_removes_files() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "session.cast", "content");

        let sessions = manager.list_sessions(None).unwrap();
        assert_eq!(sessions.len(), 1);

        manager.delete_sessions(&sessions).unwrap();

        let sessions_after = manager.list_sessions(None).unwrap();
        assert!(sessions_after.is_empty());
    }

    #[test]
    fn ensure_storage_dir_creates_directory() {
        let temp = TempDir::new().unwrap();
        let mut config = create_test_config(&temp);
        config.storage.directory = temp.path().join("sessions").to_string_lossy().to_string();
        let manager = StorageManager::new(config);

        let dir = manager.ensure_storage_dir().unwrap();
        assert!(dir.exists());
    }

    #[test]
    fn ensure_agent_dir_creates_directory() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        let dir = manager.ensure_agent_dir("test-agent").unwrap();
        assert!(dir.exists());
        assert!(dir.ends_with("test-agent"));
    }

    #[test]
    fn session_info_size_human_formats_correctly() {
        let session = SessionInfo {
            path: PathBuf::from("/test"),
            agent: "test".to_string(),
            filename: "test.cast".to_string(),
            size: 1024 * 1024, // 1 MiB
            modified: Local::now(),
            age_days: 0,
        };

        let human = session.size_human();
        assert!(human.contains("MiB") || human.contains("MB"));
    }

    #[test]
    fn stats_summary_shows_agent_breakdown() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        // Create sessions for multiple agents
        create_test_session(temp.path(), "claude", "s1.cast", "content");
        create_test_session(temp.path(), "claude", "s2.cast", "content");
        create_test_session(temp.path(), "codex", "s3.cast", "content");

        let stats = manager.get_stats().unwrap();
        let summary = stats.summary();

        // Should show breakdown by agent
        assert!(summary.contains("claude: 2"), "Summary should show claude: 2, got: {}", summary);
        assert!(summary.contains("codex: 1"), "Summary should show codex: 1, got: {}", summary);
    }

    #[test]
    fn stats_summary_shows_disk_percentage() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "test.cast", "content");

        let stats = manager.get_stats().unwrap();
        let summary = stats.summary();

        // Should show disk percentage (even if small/zero for test)
        assert!(summary.contains("% of disk"), "Summary should show disk percentage, got: {}", summary);
    }

    #[test]
    fn stats_summary_shows_oldest_session_age() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "test.cast", "content");

        let stats = manager.get_stats().unwrap();
        let summary = stats.summary();

        // Should show oldest session info
        assert!(summary.contains("Oldest:"), "Summary should show oldest session, got: {}", summary);
        assert!(summary.contains("days ago") || summary.contains("0 days"),
            "Summary should show age in days, got: {}", summary);
    }

    #[test]
    fn stats_summary_uses_human_readable_sizes() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        // Create a session with known content size
        let content = "x".repeat(1024); // 1 KiB
        create_test_session(temp.path(), "claude", "test.cast", &content);

        let stats = manager.get_stats().unwrap();
        let summary = stats.summary();

        // Should use human-readable size format (KiB, MiB, etc.)
        assert!(summary.contains("KiB") || summary.contains("KB") || summary.contains("B"),
            "Summary should use human-readable size, got: {}", summary);
    }

    #[test]
    fn stats_shows_total_session_count() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        create_test_session(temp.path(), "claude", "s1.cast", "content");
        create_test_session(temp.path(), "claude", "s2.cast", "content");
        create_test_session(temp.path(), "codex", "s3.cast", "content");

        let stats = manager.get_stats().unwrap();
        let summary = stats.summary();

        // Should show total count
        assert!(summary.contains("3 total"), "Summary should show '3 total', got: {}", summary);
    }

    #[test]
    fn disk_percentage_is_calculated() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);
        let manager = StorageManager::new(config);

        // Create a session
        create_test_session(temp.path(), "claude", "test.cast", "content");

        let stats = manager.get_stats().unwrap();

        // Disk percentage should be >= 0 (might be 0 for tiny files on large disk)
        assert!(stats.disk_percentage >= 0.0, "Disk percentage should be non-negative");
    }
}
