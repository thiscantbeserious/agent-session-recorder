//! Analysis options from CLI flags, config and per-agent config.

use super::{AgentAnalysisConfig, AgentType, AnalyzeOptions};
use crate::Config;
use anyhow::Result;

/// Parse agent name string to AgentType enum.
pub fn parse_agent_type(name: &str) -> Result<AgentType> {
    match name.to_lowercase().as_str() {
        "claude" => Ok(AgentType::Claude),
        "codex" => Ok(AgentType::Codex),
        "gemini" => Ok(AgentType::Gemini),
        _ => anyhow::bail!(
            "Unknown agent: '{}'. Supported agents: claude, codex, gemini",
            name
        ),
    }
}

/// Build analysis options with three-tier cascade: CLI > config > defaults.
#[allow(clippy::too_many_arguments)]
pub fn build_analyze_options(
    config: &Config,
    agent: AgentType,
    workers: Option<usize>,
    timeout: Option<u64>,
    no_parallel: bool,
    debug: bool,
    output: Option<String>,
    fast: bool,
) -> AnalyzeOptions {
    let mut options = AnalyzeOptions::with_agent(agent);

    // Workers: CLI > config > auto-scale (None)
    if let Some(w) = workers {
        options = options.workers(w);
    } else if let Some(w) = config.analysis.workers {
        options = options.workers(w);
    }

    // Timeout: CLI > config > default
    if let Some(t) = timeout {
        options = options.timeout(t);
    } else if let Some(t) = config.analysis.timeout {
        options = options.timeout(t);
    }

    if no_parallel {
        options = options.sequential();
    }
    if debug {
        options = options.debug(true);
    }
    if let Some(out) = output {
        options = options.output(out);
    }

    // Fast: CLI true wins, else config, else false
    if fast || config.analysis.fast.unwrap_or(false) {
        options = options.fast(true);
    }

    options
}

/// Apply per-agent extra_args and token_budget_override from agent config.
pub fn apply_agent_config(
    mut options: AnalyzeOptions,
    agent_config: Option<&AgentAnalysisConfig>,
) -> AnalyzeOptions {
    let Some(ac) = agent_config else {
        return options;
    };

    let analyze_args = ac.effective_analyze_args();
    if !analyze_args.is_empty() {
        options = options.extra_args(analyze_args.to_vec());
    }
    let curate_args = ac.effective_curate_args();
    if !curate_args.is_empty() {
        options = options.curate_extra_args(curate_args.to_vec());
    }
    let rename_args = ac.effective_rename_args();
    if !rename_args.is_empty() {
        options = options.rename_extra_args(rename_args.to_vec());
    }
    if let Some(budget) = ac.token_budget {
        options = options.token_budget_override(budget);
    }

    options
}
