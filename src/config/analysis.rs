//! Analysis configuration types for the `analyze` command.
//!
//! These are pure data containers (serde structs + validation) with no
//! analyzer-specific dependencies, so they live in the config module.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Analysis configuration for the `analyze` command.
///
/// All fields are optional so users only need to specify what they want
/// to override. CLI flags take priority over config, which overrides defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Preferred agent for analysis ("claude", "codex", "gemini")
    #[serde(default = "default_analysis_agent")]
    pub agent: Option<String>,
    /// Number of parallel workers (None = auto-scale)
    #[serde(default)]
    pub workers: Option<usize>,
    /// Timeout per chunk in seconds
    #[serde(default = "default_analysis_timeout")]
    pub timeout: Option<u64>,
    /// Fast mode (skip JSON schema enforcement)
    #[serde(default = "default_analysis_fast")]
    pub fast: Option<bool>,
    /// Auto-curate markers when count exceeds threshold
    #[serde(default = "default_analysis_curate")]
    pub curate: Option<bool>,
}

pub fn default_analysis_agent() -> Option<String> {
    None
}

pub fn default_analysis_timeout() -> Option<u64> {
    Some(120)
}

pub fn default_analysis_fast() -> Option<bool> {
    Some(false)
}

pub fn default_analysis_curate() -> Option<bool> {
    Some(true)
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            agent: default_analysis_agent(),
            workers: None,
            timeout: default_analysis_timeout(),
            fast: default_analysis_fast(),
            curate: default_analysis_curate(),
        }
    }
}

impl AnalysisConfig {
    /// Validate configuration values.
    ///
    /// Returns `Ok(())` if all values are within acceptable bounds,
    /// or an error describing the first invalid value found.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref agent) = self.agent {
            let valid = ["claude", "codex", "gemini"];
            if !valid.contains(&agent.as_str()) {
                return Err(format!(
                    "Unknown agent '{}'. Valid: {}",
                    agent,
                    valid.join(", ")
                ));
            }
        }
        if let Some(0) = self.timeout {
            return Err("analysis.timeout must be > 0".to_string());
        }
        if let Some(t) = self.timeout {
            if t > 3600 {
                return Err(format!("analysis.timeout {} exceeds maximum (3600s)", t));
            }
        }
        if let Some(0) = self.workers {
            return Err("analysis.workers must be > 0".to_string());
        }
        if let Some(w) = self.workers {
            if w > 32 {
                return Err(format!("analysis.workers {} exceeds maximum (32)", w));
            }
        }
        Ok(())
    }

    /// Validate per-agent configs (called from Config level where agents are accessible).
    pub fn validate_agent_configs(
        &self,
        agent_configs: &HashMap<String, AgentAnalysisConfig>,
    ) -> Result<(), String> {
        for (name, agent_config) in agent_configs {
            if let Some(budget) = agent_config.token_budget {
                if budget < 1000 {
                    return Err(format!(
                        "agents.{}.token_budget {} is below minimum (1000)",
                        name, budget
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Per-agent analysis configuration.
///
/// Allows customizing extra CLI arguments and token budgets for individual agents.
/// Each task type (analyze, curate, rename) can override the global `extra_args`.
///
/// ```toml
/// [agents.codex]
/// extra_args = ["--model", "gpt-5.2-codex"]                # default for all tasks
/// analyze_extra_args = ["--model", "gpt-5.2-codex"]        # override for analysis
/// curate_extra_args = ["--model", "gpt-5.1-codex-mini"]    # override for curation
/// rename_extra_args = ["--model", "gpt-5.1-codex-mini"]    # override for rename
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentAnalysisConfig {
    /// Default extra CLI arguments for all tasks
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Extra CLI arguments for analysis (overrides extra_args)
    #[serde(default)]
    pub analyze_extra_args: Vec<String>,
    /// Extra CLI arguments for curation (overrides extra_args)
    #[serde(default)]
    pub curate_extra_args: Vec<String>,
    /// Extra CLI arguments for rename (overrides extra_args)
    #[serde(default)]
    pub rename_extra_args: Vec<String>,
    /// Override the token budget for this agent
    #[serde(default)]
    pub token_budget: Option<usize>,
}

impl AgentAnalysisConfig {
    /// Get effective extra_args for analysis (analyze-specific or global fallback).
    pub fn effective_analyze_args(&self) -> &[String] {
        if self.analyze_extra_args.is_empty() {
            &self.extra_args
        } else {
            &self.analyze_extra_args
        }
    }

    /// Get effective extra_args for curation (curate-specific or global fallback).
    pub fn effective_curate_args(&self) -> &[String] {
        if self.curate_extra_args.is_empty() {
            &self.extra_args
        } else {
            &self.curate_extra_args
        }
    }

    /// Get effective extra_args for rename (rename-specific or global fallback).
    pub fn effective_rename_args(&self) -> &[String] {
        if self.rename_extra_args.is_empty() {
            &self.extra_args
        } else {
            &self.rename_extra_args
        }
    }
}
