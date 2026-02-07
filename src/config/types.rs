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
}

pub fn default_filename_template() -> String {
    "{directory}_{date}_{time}".to_string()
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
        }
    }
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_directory")]
    pub directory: String,
    #[serde(default = "default_size_threshold")]
    pub size_threshold_gb: f64,
    #[serde(default = "default_age_threshold")]
    pub age_threshold_days: u32,
}

pub fn default_directory() -> String {
    "~/recorded_agent_sessions".to_string()
}

pub fn default_size_threshold() -> f64 {
    5.0
}

pub fn default_age_threshold() -> u32 {
    30
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            directory: default_directory(),
            size_threshold_gb: default_size_threshold(),
            age_threshold_days: default_age_threshold(),
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
