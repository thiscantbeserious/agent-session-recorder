//! Configuration management for ASR

pub mod analysis;
pub mod docs;
mod io;
mod migrate;
mod types;

pub use analysis::*;
pub use migrate::*;
pub use types::*;

use anyhow::Result;
use std::path::PathBuf;

use crate::analyzer::backend::command_exists;

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

    /// Resolve the analysis agent with cascade:
    /// 1. `[analysis].agent` (explicit config)
    /// 2. Auto-detect first available agent binary on PATH
    /// 3. Fall back to "claude"
    pub fn resolve_analysis_agent(&self) -> String {
        // 1. Prefer explicit [analysis].agent
        if let Some(ref agent) = self.analysis.agent {
            return agent.clone();
        }

        // 2. Auto-detect first available agent binary
        for (cmd, name) in &[
            ("claude", "claude"),
            ("codex", "codex"),
            ("gemini", "gemini"),
        ] {
            if command_exists(cmd) {
                return name.to_string();
            }
        }

        // 3. Ultimate fallback
        "claude".to_string()
    }

    /// Look up per-agent analysis configuration.
    ///
    /// Returns `None` if the agent name is not recognized.
    pub fn analysis_agent_config(&self, agent_name: &str) -> Option<&AgentAnalysisConfig> {
        self.agents.agent_config(agent_name)
    }
}
