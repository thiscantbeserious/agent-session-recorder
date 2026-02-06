//! Configuration management for ASR

mod io;
mod migrate;
mod types;

pub use migrate::*;
pub use types::*;

use anyhow::Result;
use std::path::PathBuf;

impl Config {
    /// Get the config file path (~/.config/agr/config.toml)
    pub fn config_path() -> Result<PathBuf> {
        io::config_path()
    }

    /// Get the config directory path (~/.config/agr)
    pub fn config_dir() -> Result<PathBuf> {
        io::config_dir()
    }

    /// Load configuration from file, or return defaults if not found
    pub fn load() -> Result<Self> {
        io::load()
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        io::save(self)
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

    /// Resolve the analysis agent, preferring `[analysis].default_agent` over
    /// the deprecated `[recording].analysis_agent`.
    ///
    /// If neither is set, returns the default ("claude").
    /// Emits a deprecation warning to stderr when the old field is being used.
    pub fn resolve_analysis_agent(&self) -> String {
        // Prefer new [analysis].default_agent
        if let Some(ref agent) = self.analysis.default_agent {
            return agent.clone();
        }

        // Fall back to deprecated [recording].analysis_agent
        let old_value = &self.recording.analysis_agent;
        let default_value = default_analysis_agent();

        if *old_value != default_value {
            eprintln!(
                "Warning: [recording].analysis_agent is deprecated. \
                 Use [analysis].default_agent instead."
            );
        }

        old_value.clone()
    }

    /// Look up per-agent analysis configuration.
    ///
    /// Returns `None` if no agent-specific config exists.
    pub fn analysis_agent_config(&self, agent_name: &str) -> Option<&AgentAnalysisConfig> {
        self.analysis.agents.get(agent_name)
    }
}
