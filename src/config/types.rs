//! Configuration type definitions and defaults

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use super::analysis::{AgentAnalysisConfig, AnalysisConfig};
use crate::config::migrate::CURRENT_VERSION;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Schema version — used by the migration system to track applied migrations.
    #[serde(default = "default_config_version")]
    pub config_version: u32,
    #[serde(default)]
    pub shell: ShellConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub recording: RecordingConfig,
    #[serde(default)]
    pub analysis: AnalysisConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

fn default_config_version() -> u32 {
    CURRENT_VERSION
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_version: CURRENT_VERSION,
            shell: ShellConfig::default(),
            storage: StorageConfig::default(),
            recording: RecordingConfig::default(),
            analysis: AnalysisConfig::default(),
            agents: AgentsConfig::default(),
            sync: SyncConfig::default(),
        }
    }
}

/// Session sync configuration. Sync uses `gist` unless `backend = "off"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default = "default_backend")]
    pub backend: SyncBackendKind,
    /// Push new sessions automatically after recording (unless the backend is `off`).
    #[serde(default = "default_auto_push")]
    pub auto_push: bool,
}

/// Remote backend for session sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncBackendKind {
    Gist,
    Off,
}

pub fn default_backend() -> SyncBackendKind {
    SyncBackendKind::Gist
}

pub fn default_auto_push() -> bool {
    true
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            auto_push: default_auto_push(),
        }
    }
}

/// Shell integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    /// Global toggle for auto-wrapping agents
    #[serde(default = "default_auto_wrap")]
    pub auto_wrap: bool,
    /// Path to the shell script (computed, not stored in config)
    #[serde(skip)]
    pub script_path: Option<PathBuf>,
}

pub fn default_auto_wrap() -> bool {
    true
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            auto_wrap: default_auto_wrap(),
            script_path: None,
        }
    }
}

/// Recording configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingConfig {
    /// Whether to automatically analyze the recording after session ends
    #[serde(default)]
    pub auto_analyze: bool,
    /// Filename template using tags like {directory}, {date}, {time}
    #[serde(default = "default_filename_template")]
    pub filename_template: String,
    /// Maximum length for directory component in filename
    #[serde(default = "default_directory_max_length")]
    pub directory_max_length: usize,
    /// Remove idle time from each recording after it ends (no backup is kept)
    #[serde(default = "default_auto_optimize")]
    pub auto_optimize: bool,
}

pub fn default_auto_optimize() -> bool {
    true
}

pub fn default_filename_template() -> String {
    "{directory}{?branch}_{id}".to_string()
}

pub fn default_directory_max_length() -> usize {
    14
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            auto_analyze: false,
            filename_template: default_filename_template(),
            directory_max_length: default_directory_max_length(),
            auto_optimize: default_auto_optimize(),
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_directory")]
    pub directory: String,
    /// Warn after recording when storage exceeds this size. Unset disables the warning.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_threshold_gb: Option<f64>,
    #[serde(default = "default_age_threshold")]
    pub age_threshold_days: u32,
    /// Compression for new recordings. Existing files keep their format.
    #[serde(default)]
    pub compression: Compression,
}

/// On-disk format of new recordings: `.cast.gz` for gzip, `.cast` for none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    #[default]
    Gzip,
    None,
}

pub fn default_directory() -> String {
    "~/recorded_agent_sessions".to_string()
}

pub fn default_age_threshold() -> u32 {
    30
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            directory: default_directory(),
            size_threshold_gb: None,
            age_threshold_days: default_age_threshold(),
            compression: Compression::default(),
        }
    }
}

/// Agents configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    #[serde(default = "default_agents")]
    pub enabled: Vec<String>,
    /// Agents that should not be auto-wrapped (even if in enabled list)
    #[serde(default)]
    pub no_wrap: Vec<String>,
    /// Per-agent analysis configuration (e.g. extra CLI args, token budget)
    #[serde(default)]
    pub claude: AgentAnalysisConfig,
    #[serde(default)]
    pub codex: AgentAnalysisConfig,
    #[serde(default)]
    pub gemini: AgentAnalysisConfig,
}

pub fn default_agents() -> Vec<String> {
    vec![
        "claude".to_string(),
        "codex".to_string(),
        "gemini".to_string(),
    ]
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            enabled: default_agents(),
            no_wrap: Vec::new(),
            claude: AgentAnalysisConfig::default(),
            codex: AgentAnalysisConfig::default(),
            gemini: AgentAnalysisConfig::default(),
        }
    }
}

impl AgentsConfig {
    /// Look up per-agent analysis configuration by name.
    pub fn agent_config(&self, name: &str) -> Option<&AgentAnalysisConfig> {
        match name {
            "claude" => Some(&self.claude),
            "codex" => Some(&self.codex),
            "gemini" => Some(&self.gemini),
            _ => None,
        }
    }

    /// Get all per-agent configs as a HashMap (for validation).
    pub fn agent_configs_map(&self) -> HashMap<String, &AgentAnalysisConfig> {
        let mut map = HashMap::new();
        map.insert("claude".to_string(), &self.claude);
        map.insert("codex".to_string(), &self.codex);
        map.insert("gemini".to_string(), &self.gemini);
        map
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, SyncBackendKind, SyncConfig};

    #[test]
    fn sync_backend_defaults_to_gist() {
        assert_eq!(SyncConfig::default().backend, SyncBackendKind::Gist);
        let parsed: Config = toml::from_str("[sync]\nauto_push = false\n").unwrap();
        assert_eq!(parsed.sync.backend, SyncBackendKind::Gist);
    }

    #[test]
    fn auto_push_is_off_by_default() {
        assert!(!SyncConfig::default().auto_push);
        let parsed: Config = toml::from_str("[sync]\n").unwrap();
        assert!(!parsed.sync.auto_push);
    }

    #[test]
    fn backend_off_disables_sync() {
        let parsed: Config = toml::from_str("[sync]\nbackend = \"off\"\n").unwrap();
        assert_eq!(parsed.sync.backend, SyncBackendKind::Off);
    }
}
