//! Storage management for recorded sessions
//!
//! Provides functionality for:
//! - Listing and filtering session recordings
//! - Importing external .cast files into managed storage
//! - Managing agent directories and session metadata
//! - Computing storage statistics

pub mod compress;
mod import;
pub mod migrate;
mod sessions;
mod stats;

pub use import::{validate_cast_header, ImportError};
pub use sessions::{format_smart_time, format_uncompressed, SessionInfo};
pub use stats::StorageStats;

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::files::remove_auxiliary_files;

/// Storage manager for session recordings
pub struct StorageManager {
    config: Config,
}

impl StorageManager {
    /// Create a new storage manager with the given config.
    ///
    /// Runs a migration sweep to rename old-format auxiliary files (lock/backup)
    /// to the hidden dot-prefix format before any storage operation.
    pub fn new(config: Config) -> Self {
        let storage_dir = config.storage_directory();
        let result = migrate::execute(&storage_dir);
        if result.files_renamed > 0 || result.files_failed > 0 {
            eprintln!(
                "Storage migration: {} file(s) renamed to hidden format{}",
                result.files_renamed,
                if result.files_failed > 0 {
                    format!(", {} failed", result.files_failed)
                } else {
                    String::new()
                }
            );
            for warning in &result.warnings {
                eprintln!("  warning: {}", warning);
            }
        }
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

    /// Delete sessions by path.
    ///
    /// Removes the `.cast` file and all associated auxiliary files (lock and backup,
    /// both hidden and legacy formats) as best-effort cleanup.
    pub fn delete_sessions(&self, sessions: &[SessionInfo]) -> Result<u64> {
        let mut freed_size = 0u64;

        for session in sessions {
            // Always attempt auxiliary cleanup, even if the cast file is already gone
            remove_auxiliary_files(&session.path);
            if session.path.exists() {
                fs::remove_file(&session.path)
                    .with_context(|| format!("Failed to delete: {:?}", session.path))?;
                freed_size += session.size;
            }
        }

        Ok(freed_size)
    }

    /// Check if storage exceeds the configured threshold (false when unset)
    pub fn exceeds_threshold(&self) -> Result<bool> {
        let Some(threshold_gb) = self.config.storage.size_threshold_gb else {
            return Ok(false);
        };
        let stats = self.get_stats()?;
        let threshold_bytes = (threshold_gb * 1024.0 * 1024.0 * 1024.0) as u64;
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
