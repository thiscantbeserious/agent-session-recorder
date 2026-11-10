//! Tests for the analyze option builders.

use agr::analyzer::options::{apply_agent_config, build_analyze_options, parse_agent_type};
use agr::analyzer::{AgentType, AnalyzeOptions};
use agr::config::{AgentAnalysisConfig, AnalysisConfig};
use agr::Config;

// ---- parse_agent_type ----

#[test]
fn parse_agent_type_claude() {
    assert_eq!(parse_agent_type("claude").unwrap(), AgentType::Claude);
    assert_eq!(parse_agent_type("CLAUDE").unwrap(), AgentType::Claude);
}

#[test]
fn parse_agent_type_codex() {
    assert_eq!(parse_agent_type("codex").unwrap(), AgentType::Codex);
}

#[test]
fn parse_agent_type_gemini() {
    assert_eq!(parse_agent_type("gemini").unwrap(), AgentType::Gemini);
}

#[test]
fn parse_agent_type_unknown() {
    assert!(parse_agent_type("unknown").is_err());
}

// ---- helpers ----

fn default_config() -> Config {
    Config::default()
}

fn config_with_analysis(analysis: AnalysisConfig) -> Config {
    Config {
        analysis,
        ..Config::default()
    }
}

// ---- build_analyze_options ----

#[test]
fn build_analyze_options_all_defaults() {
    let config = default_config();
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        None,
        None,
        false,
        false,
        None,
        false,
    );
    assert_eq!(opts.agent, AgentType::Claude);
    assert_eq!(opts.workers, None);
    assert!(!opts.no_parallel);
    assert!(!opts.debug);
    assert_eq!(opts.output_path, None);
    assert!(!opts.fast);
}

#[test]
fn build_analyze_options_cli_workers_wins_over_config() {
    let analysis = AnalysisConfig {
        workers: Some(2),
        ..Default::default()
    };
    let config = config_with_analysis(analysis);
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        Some(8),
        None,
        false,
        false,
        None,
        false,
    );
    assert_eq!(opts.workers, Some(8));
}

#[test]
fn build_analyze_options_config_workers_fallback() {
    let analysis = AnalysisConfig {
        workers: Some(4),
        ..Default::default()
    };
    let config = config_with_analysis(analysis);
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        None,
        None,
        false,
        false,
        None,
        false,
    );
    assert_eq!(opts.workers, Some(4));
}

#[test]
fn build_analyze_options_cli_timeout_wins_over_config() {
    let analysis = AnalysisConfig {
        timeout: Some(60),
        ..Default::default()
    };
    let config = config_with_analysis(analysis);
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        None,
        Some(300),
        false,
        false,
        None,
        false,
    );
    assert_eq!(opts.timeout_secs, 300);
}

#[test]
fn build_analyze_options_config_timeout_fallback() {
    let analysis = AnalysisConfig {
        timeout: Some(45),
        ..Default::default()
    };
    let config = config_with_analysis(analysis);
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        None,
        None,
        false,
        false,
        None,
        false,
    );
    assert_eq!(opts.timeout_secs, 45);
}

#[test]
fn build_analyze_options_no_parallel_sets_sequential() {
    let config = default_config();
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        None,
        None,
        true,
        false,
        None,
        false,
    );
    assert!(opts.no_parallel);
}

#[test]
fn build_analyze_options_config_fast_enables_fast() {
    let analysis = AnalysisConfig {
        fast: Some(true),
        ..Default::default()
    };
    let config = config_with_analysis(analysis);
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        None,
        None,
        false,
        false,
        None,
        false,
    );
    assert!(opts.fast);
}

#[test]
fn build_analyze_options_cli_fast_enables_fast() {
    let config = default_config();
    let opts = build_analyze_options(
        &config,
        AgentType::Claude,
        None,
        None,
        false,
        false,
        None,
        true,
    );
    assert!(opts.fast);
}

#[test]
fn build_analyze_options_debug_and_output() {
    let config = default_config();
    let opts = build_analyze_options(
        &config,
        AgentType::Gemini,
        None,
        None,
        false,
        true,
        Some("/tmp/out.txt".to_string()),
        false,
    );
    assert!(opts.debug);
    assert_eq!(opts.output_path, Some("/tmp/out.txt".to_string()));
    assert_eq!(opts.agent, AgentType::Gemini);
}

// ---- apply_agent_config ----

#[test]
fn apply_agent_config_none_passthrough() {
    let base = AnalyzeOptions::with_agent(AgentType::Claude).workers(3);
    let opts = apply_agent_config(base, None);
    assert_eq!(opts.workers, Some(3));
    assert!(opts.extra_args.is_empty());
    assert_eq!(opts.token_budget_override, None);
}

#[test]
fn apply_agent_config_global_args_fallback() {
    let base = AnalyzeOptions::with_agent(AgentType::Claude);
    let ac = AgentAnalysisConfig {
        extra_args: vec!["--verbose".to_string()],
        ..AgentAnalysisConfig::default()
    };
    let opts = apply_agent_config(base, Some(&ac));
    assert_eq!(opts.extra_args, vec!["--verbose"]);
}

#[test]
fn apply_agent_config_task_specific_analyze_override() {
    let base = AnalyzeOptions::with_agent(AgentType::Claude);
    let ac = AgentAnalysisConfig {
        extra_args: vec!["--global".to_string()],
        analyze_extra_args: vec!["--analyze-only".to_string()],
        ..AgentAnalysisConfig::default()
    };
    let opts = apply_agent_config(base, Some(&ac));
    // analyze_extra_args should win over extra_args for the analysis task
    assert_eq!(opts.extra_args, vec!["--analyze-only"]);
}

#[test]
fn apply_agent_config_curate_args_set() {
    let base = AnalyzeOptions::with_agent(AgentType::Codex);
    let ac = AgentAnalysisConfig {
        curate_extra_args: vec!["--curate-flag".to_string()],
        ..AgentAnalysisConfig::default()
    };
    let opts = apply_agent_config(base, Some(&ac));
    assert_eq!(opts.curate_extra_args, vec!["--curate-flag"]);
}

#[test]
fn apply_agent_config_token_budget_set() {
    let base = AnalyzeOptions::with_agent(AgentType::Claude);
    let ac = AgentAnalysisConfig {
        token_budget: Some(50_000),
        ..AgentAnalysisConfig::default()
    };
    let opts = apply_agent_config(base, Some(&ac));
    assert_eq!(opts.token_budget_override, Some(50_000));
}

#[test]
fn apply_agent_config_all_empty_agent_config() {
    let base = AnalyzeOptions::with_agent(AgentType::Gemini).timeout(90);
    let ac = AgentAnalysisConfig::default();
    let opts = apply_agent_config(base, Some(&ac));
    // Empty config should not change anything
    assert!(opts.extra_args.is_empty());
    assert!(opts.curate_extra_args.is_empty());
    assert_eq!(opts.token_budget_override, None);
    assert_eq!(opts.timeout_secs, 90);
}

#[test]
fn apply_agent_config_global_args_fallback_into_curate_and_rename() {
    let base = AnalyzeOptions::with_agent(AgentType::Claude);
    let ac = AgentAnalysisConfig {
        extra_args: vec!["--global".to_string()],
        ..AgentAnalysisConfig::default()
    };
    let opts = apply_agent_config(base, Some(&ac));
    assert_eq!(opts.extra_args, vec!["--global"]);
    assert_eq!(opts.curate_extra_args, vec!["--global"]);
    assert_eq!(opts.rename_extra_args, vec!["--global"]);
}

#[test]
fn apply_agent_config_rename_args_set() {
    let base = AnalyzeOptions::with_agent(AgentType::Claude);
    let ac = AgentAnalysisConfig {
        rename_extra_args: vec!["--rename-flag".to_string()],
        ..AgentAnalysisConfig::default()
    };
    let opts = apply_agent_config(base, Some(&ac));
    assert_eq!(opts.rename_extra_args, vec!["--rename-flag"]);
    // Curate should not be affected
    assert!(opts.curate_extra_args.is_empty());
}
