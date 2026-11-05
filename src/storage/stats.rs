//! Storage statistics

use super::{format_uncompressed, SessionInfo, StorageManager};
use crate::files::codec::UncompressedSize;
use anyhow::Result;
use humansize::{format_size, BINARY};
use std::collections::HashMap;

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_size: u64,
    pub uncompressed_total: UncompressedSize,
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

    /// Get human-readable uncompressed total
    pub fn uncompressed_human(&self) -> String {
        format_uncompressed(self.uncompressed_total)
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
            "Agent Sessions: {} on disk, {} uncompressed ({:.2}% of disk)\n   Sessions: {} total{}",
            self.size_human(),
            self.uncompressed_human(),
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

/// Sum of uncompressed sizes, `Over4GiB` once any part overflowed. A file whose header cannot
/// be read counts with its on-disk size.
fn sum_uncompressed(sessions: &[SessionInfo]) -> UncompressedSize {
    let mut total = 0u64;
    for session in sessions {
        match session.uncompressed_size() {
            Ok(UncompressedSize::Over4GiB) => return UncompressedSize::Over4GiB,
            Ok(UncompressedSize::Exact(n)) => total = total.saturating_add(n),
            Err(_) => total = total.saturating_add(session.size),
        }
    }
    UncompressedSize::Exact(total)
}

impl StorageManager {
    /// Get storage statistics
    pub fn get_stats(&self) -> Result<StorageStats> {
        let sessions = self.list_sessions(None)?;

        let total_size: u64 = sessions.iter().map(|s| s.size).sum();
        let uncompressed_total = sum_uncompressed(&sessions);
        let session_count = sessions.len();

        let mut sessions_by_agent: HashMap<String, usize> = HashMap::new();
        for session in &sessions {
            *sessions_by_agent.entry(session.agent.clone()).or_insert(0) += 1;
        }

        let oldest_session = sessions.last().cloned();

        // Calculate disk percentage (simplified - uses available space)
        let disk_percentage = self.calculate_disk_percentage(total_size);

        Ok(StorageStats {
            total_size,
            uncompressed_total,
            session_count,
            sessions_by_agent,
            oldest_session,
            disk_percentage,
        })
    }

    /// Calculate what percentage of disk the storage uses
    fn calculate_disk_percentage(&self, total_size: u64) -> f64 {
        let storage_dir = self.storage_dir();
        let path_str = storage_dir.to_string_lossy();
        query_disk_total_bytes(&path_str)
            .map(|total_bytes| (total_size as f64 / total_bytes as f64) * 100.0)
            .unwrap_or(0.0)
    }
}

/// Query total disk bytes for the given path using `df -k`.
///
/// Returns `None` if the command fails or the output cannot be parsed.
fn query_disk_total_bytes(path: &str) -> Option<u64> {
    let output = std::process::Command::new("df")
        .arg("-k")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse df output — second line, second column is total 1K-blocks
    let line = stdout.lines().nth(1)?;
    let total_kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    if total_kb == 0 {
        return None;
    }
    Some(total_kb * 1024)
}
