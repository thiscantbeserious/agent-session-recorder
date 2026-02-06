//! Configuration for the content extraction pipeline and analysis service.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the content extraction pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionConfig {
    /// Strip ANSI escape sequences (always true)
    pub strip_ansi: bool,
    /// Strip control characters (always true)
    pub strip_control_chars: bool,
    /// Deduplicate progress lines using \r
    pub dedupe_progress_lines: bool,
    /// Normalize excessive whitespace
    pub normalize_whitespace: bool,
    /// Maximum consecutive newlines allowed
    pub max_consecutive_newlines: usize,
    /// Strip box drawing characters
    pub strip_box_drawing: bool,
    /// Strip spinner animation characters
    pub strip_spinner_chars: bool,
    /// Strip progress bar block characters
    pub strip_progress_blocks: bool,
    /// Time gap threshold for segment boundaries (seconds)
    pub segment_time_gap: f64,
    /// Enable similarity-based line collapsing (targets redundant log lines)
    pub collapse_similar_lines: bool,
    /// Similarity threshold (0.0 to 1.0) for collapsing lines
    pub similarity_threshold: f64,
    /// Enable coalescing of rapid, similar events (targets TUI redrawing)
    pub coalesce_events: bool,
    /// Time threshold for event coalescing (seconds)
    pub coalesce_time_threshold: f64,
    /// Enable truncation of large output blocks
    pub truncate_large_blocks: bool,
    /// Max times a specific line can repeat globally across the session
    pub max_line_repeats: usize,
    /// Window size for event hashing (number of events to check for redraws)
    pub event_window_size: usize,
    /// Maximum number of lines in a burst before it's considered a file dump
    pub max_burst_lines: usize,
    /// Maximum size of an output block before truncation (bytes)
    pub max_block_size: usize,
    /// Number of lines to keep at head/tail during truncation
    pub truncation_context_lines: usize,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            strip_ansi: true,
            strip_control_chars: true,
            dedupe_progress_lines: false,
            normalize_whitespace: true,
            max_consecutive_newlines: 2,
            strip_box_drawing: true,
            strip_spinner_chars: true,
            strip_progress_blocks: true,
            segment_time_gap: 2.0,
            collapse_similar_lines: true,
            similarity_threshold: 0.80,
            coalesce_events: true,
            coalesce_time_threshold: 0.2, // 200ms
            max_line_repeats: 10,
            event_window_size: 50,
            max_burst_lines: 500,
            truncate_large_blocks: true,
            max_block_size: 8 * 1024, // 8KB
            truncation_context_lines: 50,
        }
    }
}

/// Analysis configuration for the `analyze` command.
///
/// All fields are optional so users only need to specify what they want
/// to override. CLI flags take priority over config, which overrides defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    /// Default agent for analysis ("claude", "codex", "gemini")
    #[serde(default = "default_analysis_default_agent")]
    pub default_agent: Option<String>,
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
    /// Per-agent configuration overrides
    #[serde(default = "default_analysis_agents")]
    pub agents: HashMap<String, AgentAnalysisConfig>,
}

pub fn default_analysis_default_agent() -> Option<String> {
    Some("claude".to_string())
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

pub fn default_analysis_agents() -> HashMap<String, AgentAnalysisConfig> {
    let mut agents = HashMap::new();
    agents.insert("claude".to_string(), AgentAnalysisConfig::default());
    agents.insert("codex".to_string(), AgentAnalysisConfig::default());
    agents.insert("gemini".to_string(), AgentAnalysisConfig::default());
    agents
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            default_agent: default_analysis_default_agent(),
            workers: None,
            timeout: default_analysis_timeout(),
            fast: default_analysis_fast(),
            curate: default_analysis_curate(),
            agents: default_analysis_agents(),
        }
    }
}

/// Per-agent analysis configuration.
///
/// Allows customizing extra CLI arguments and token budgets for individual agents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentAnalysisConfig {
    /// Extra CLI arguments to pass to the agent
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Override the token budget for this agent
    #[serde(default)]
    pub token_budget: Option<usize>,
}