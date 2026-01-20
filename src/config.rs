//! Configuration management for ASR

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
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
        "gemini-cli".to_string(),
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
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            storage: StorageConfig::default(),
            agents: AgentsConfig::default(),
        }
    }
}

impl Config {
    /// Get the config file path (~/.config/asr/config.toml)
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = Self::config_dir()?;
        Ok(config_dir.join("config.toml"))
    }

    /// Get the config directory path (~/.config/asr)
    pub fn config_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not determine home directory")?;
        Ok(home.join(".config").join("asr"))
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

        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(&config_path, contents)
            .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

        Ok(())
    }

    /// Expand ~ in storage directory path
    pub fn storage_directory(&self) -> PathBuf {
        let dir = &self.storage.directory;
        if dir.starts_with("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(&dir[2..]);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = Config::default();
        assert_eq!(config.storage.directory, "~/recorded_agent_sessions");
        assert_eq!(config.storage.size_threshold_gb, 5.0);
        assert_eq!(config.storage.age_threshold_days, 30);
        assert!(config.agents.enabled.contains(&"claude".to_string()));
        assert!(config.agents.enabled.contains(&"codex".to_string()));
        assert!(config.agents.enabled.contains(&"gemini-cli".to_string()));
    }

    #[test]
    fn config_serialization_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.storage.directory, config.storage.directory);
        assert_eq!(parsed.agents.enabled, config.agents.enabled);
    }

    #[test]
    fn add_agent_adds_new_agent() {
        let mut config = Config::default();
        assert!(config.add_agent("new-agent"));
        assert!(config.is_agent_enabled("new-agent"));
    }

    #[test]
    fn add_agent_does_not_duplicate() {
        let mut config = Config::default();
        assert!(!config.add_agent("claude"));
        assert_eq!(
            config.agents.enabled.iter().filter(|a| *a == "claude").count(),
            1
        );
    }

    #[test]
    fn remove_agent_removes_existing() {
        let mut config = Config::default();
        assert!(config.remove_agent("claude"));
        assert!(!config.is_agent_enabled("claude"));
    }

    #[test]
    fn remove_agent_returns_false_for_nonexistent() {
        let mut config = Config::default();
        assert!(!config.remove_agent("nonexistent"));
    }

    #[test]
    fn storage_directory_expands_tilde() {
        let config = Config::default();
        let path = config.storage_directory();
        assert!(!path.to_string_lossy().contains('~'));
        assert!(path.to_string_lossy().contains("recorded_agent_sessions"));
    }
}
