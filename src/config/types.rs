//! Configuration type definitions and defaults

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::analyzer::AnalysisConfig;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub shell: ShellConfig,
    #[serde(default)]
    pub recording: RecordingConfig,
    #[serde(default)]
    pub analysis: AnalysisConfig,
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
    /// Which agent to use for analysis ("claude", "codex", "gemini")
    #[serde(default = "default_analysis_agent")]
    pub analysis_agent: String,
    /// Filename template using tags like {directory}, {date}, {time}
    #[serde(default = "default_filename_template")]
    pub filename_template: String,
    /// Maximum length for directory component in filename (default: 50)
    #[serde(default = "default_directory_max_length")]
    pub directory_max_length: usize,
}

pub fn default_analysis_agent() -> String {
    "claude".to_string()
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
            analysis_agent: default_analysis_agent(),
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
        }
    }
}