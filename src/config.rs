//! Configuration management for ASR

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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

fn default_auto_wrap() -> bool {
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
}

fn default_analysis_agent() -> String {
    "claude".to_string()
}

impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            auto_analyze: false,
            analysis_agent: default_analysis_agent(),
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

/// Agents configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentsConfig {
    #[serde(default = "default_agents")]
    pub enabled: Vec<String>,
    /// Agents that should not be auto-wrapped (even if in enabled list)
    #[serde(default)]
    pub no_wrap: Vec<String>,
}

fn default_directory() -> String {
    "~/recorded_agent_sessions".to_string()
}

fn default_size_threshold() -> f64 {
    5.0
}

fn default_age_threshold() -> u32 {
    30
}

fn default_agents() -> Vec<String> {
    vec![
        "claude".to_string(),
        "codex".to_string(),
        "gemini".to_string(),
    ]
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

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            enabled: default_agents(),
            no_wrap: Vec::new(),
        }
    }
}

impl Config {
    /// Get the config file path (~/.config/agr/config.toml)
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join("config.toml"))
    }

    /// Get the config directory path (~/.config/agr)
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".config").join("agr"))
    }

    /// Load configuration from file, or return defaults if not found
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let contents = fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
            let config: Config = toml::from_str(&contents)
                .with_context(|| format!("Failed to parse config file: {:?}", config_path))?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        // Ensure config directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
        }

        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_path, contents)
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

        Ok(())
    }

    /// Expand ~ in storage directory path
    pub fn storage_directory(&self) -> PathBuf {
        let dir = &self.storage.directory;
        if let Some(stripped) = dir.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        PathBuf::from(dir)
    }

    /// Add an agent to the enabled list
    pub fn add_agent(&mut self, name: &str) -> bool {
        let name = name.to_string();
        if !self.agents.enabled.contains(&name) {
            self.agents.enabled.push(name);
            true
        } else {
            false
        }
    }

    /// Remove an agent from the enabled list
    pub fn remove_agent(&mut self, name: &str) -> bool {
        let initial_len = self.agents.enabled.len();
        self.agents.enabled.retain(|a| a != name);
        self.agents.enabled.len() < initial_len
    }

    /// Check if an agent is enabled
    pub fn is_agent_enabled(&self, name: &str) -> bool {
        self.agents.enabled.contains(&name.to_string())
    }

    /// Check if an agent should be wrapped (enabled and not in no_wrap list)
    pub fn should_wrap_agent(&self, name: &str) -> bool {
        self.shell.auto_wrap
            && self.is_agent_enabled(name)
            && !self.agents.no_wrap.contains(&name.to_string())
    }

    /// Add an agent to the no_wrap list
    pub fn add_no_wrap(&mut self, name: &str) -> bool {
        let name = name.to_string();
        if !self.agents.no_wrap.contains(&name) {
            self.agents.no_wrap.push(name);
            true
        } else {
            false
        }
    }

    /// Remove an agent from the no_wrap list
    pub fn remove_no_wrap(&mut self, name: &str) -> bool {
        let initial_len = self.agents.no_wrap.len();
        self.agents.no_wrap.retain(|a| a != name);
        self.agents.no_wrap.len() < initial_len
    }
}
