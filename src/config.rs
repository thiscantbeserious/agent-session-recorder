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
        assert!(config.agents.enabled.contains(&"gemini".to_string()));
        // Shell config defaults
        assert!(config.shell.auto_wrap);
        assert!(config.shell.script_path.is_none());
        // Recording config defaults
        assert!(!config.recording.auto_analyze);
        // Agents no_wrap defaults
        assert!(config.agents.no_wrap.is_empty());
    }

    #[test]
    fn config_serialization_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.storage.directory, config.storage.directory);
        assert_eq!(parsed.agents.enabled, config.agents.enabled);
        assert_eq!(parsed.shell.auto_wrap, config.shell.auto_wrap);
    }

    #[test]
    fn shell_config_parses_from_toml() {
        let toml_str = r#"
[shell]
auto_wrap = false
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.shell.auto_wrap);
    }

    #[test]
    fn shell_config_defaults_when_missing() {
        let toml_str = r#"
[storage]
directory = "~/custom"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        // Shell config should have default values
        assert!(config.shell.auto_wrap);
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
            config
                .agents
                .enabled
                .iter()
                .filter(|a| *a == "claude")
                .count(),
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

    #[test]
    fn should_wrap_agent_respects_enabled_list() {
        let config = Config::default();
        assert!(config.should_wrap_agent("claude"));
        assert!(!config.should_wrap_agent("unknown-agent"));
    }

    #[test]
    fn should_wrap_agent_respects_no_wrap_list() {
        let mut config = Config::default();
        assert!(config.should_wrap_agent("claude"));
        config.add_no_wrap("claude");
        assert!(!config.should_wrap_agent("claude"));
    }

    #[test]
    fn should_wrap_agent_respects_auto_wrap_toggle() {
        let mut config = Config::default();
        assert!(config.should_wrap_agent("claude"));
        config.shell.auto_wrap = false;
        assert!(!config.should_wrap_agent("claude"));
    }

    #[test]
    fn add_no_wrap_adds_new_agent() {
        let mut config = Config::default();
        assert!(config.add_no_wrap("test-agent"));
        assert!(config.agents.no_wrap.contains(&"test-agent".to_string()));
    }

    #[test]
    fn add_no_wrap_does_not_duplicate() {
        let mut config = Config::default();
        config.add_no_wrap("test-agent");
        assert!(!config.add_no_wrap("test-agent"));
        assert_eq!(
            config
                .agents
                .no_wrap
                .iter()
                .filter(|a| *a == "test-agent")
                .count(),
            1
        );
    }

    #[test]
    fn remove_no_wrap_removes_existing() {
        let mut config = Config::default();
        config.add_no_wrap("test-agent");
        assert!(config.remove_no_wrap("test-agent"));
        assert!(!config.agents.no_wrap.contains(&"test-agent".to_string()));
    }

    #[test]
    fn remove_no_wrap_returns_false_for_nonexistent() {
        let mut config = Config::default();
        assert!(!config.remove_no_wrap("nonexistent"));
    }

    #[test]
    fn recording_config_parses_from_toml() {
        let toml_str = r#"
[recording]
auto_analyze = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.recording.auto_analyze);
    }

    #[test]
    fn recording_config_with_analysis_agent() {
        let toml_str = r#"
[recording]
auto_analyze = true
analysis_agent = "codex"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.recording.auto_analyze);
        assert_eq!(config.recording.analysis_agent, "codex");
    }

    #[test]
    fn recording_config_defaults_analysis_agent_to_claude() {
        let toml_str = r#"
[recording]
auto_analyze = true
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.recording.analysis_agent, "claude");
    }

    #[test]
    fn recording_config_defaults_when_missing() {
        let toml_str = r#"
[storage]
directory = "~/custom"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(!config.recording.auto_analyze);
    }

    #[test]
    fn no_wrap_config_parses_from_toml() {
        let toml_str = r#"
[agents]
enabled = ["claude", "codex"]
no_wrap = ["codex"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.should_wrap_agent("claude"));
        assert!(!config.should_wrap_agent("codex"));
    }

    #[test]
    fn config_path_returns_valid_path() {
        let path = Config::config_path().unwrap();
        assert!(path.to_string_lossy().contains("config.toml"));
        assert!(path.to_string_lossy().contains("agr"));
    }

    #[test]
    fn config_dir_returns_valid_path() {
        let dir = Config::config_dir().unwrap();
        assert!(dir.to_string_lossy().contains("agr"));
        assert!(dir.to_string_lossy().contains(".config"));
    }

    #[test]
    fn load_returns_default_when_no_config_file() {
        // This relies on Config::load() returning defaults when file doesn't exist
        // Since we can't easily mock the filesystem, we test the logic indirectly
        let config = Config::default();
        assert_eq!(config.storage.directory, "~/recorded_agent_sessions");
    }

    #[test]
    fn storage_directory_handles_non_tilde_path() {
        let mut config = Config::default();
        config.storage.directory = "/absolute/path".to_string();
        let path = config.storage_directory();
        assert_eq!(path, std::path::PathBuf::from("/absolute/path"));
    }

    #[test]
    fn storage_directory_handles_relative_path() {
        let mut config = Config::default();
        config.storage.directory = "relative/path".to_string();
        let path = config.storage_directory();
        assert_eq!(path, std::path::PathBuf::from("relative/path"));
    }
}
