//! Analyze command handler
//!
//! Uses the AnalyzerService facade to orchestrate analysis:
//! 1. Parse cast file
//! 2. Check for existing markers (offer to remove)
//! 3. Extract content (strip ANSI, dedupe progress)
//! 4. Chunk content based on agent token limits
//! 5. Execute parallel analysis
//! 6. Aggregate and deduplicate markers
//! 7. Write markers to file
//! 8. Optionally curate markers (reduce to 8-12 most significant)
//! 9. Suggest better filename via LLM based on analysis

use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use agr::analyzer::{
    AgentAnalysisConfig, AgentType, AnalyzeOptions, AnalyzerService, ValidatedMarker,
};
use agr::{Config, MarkerManager};

use agr::asciicast::integrity::check_file_integrity;

/// Threshold for offering marker curation.
const CURATION_THRESHOLD: usize = 12;

/// Command line values of `agr analyze`, built once in dispatch.
pub struct AnalyzeArgs {
    pub file: String,
    pub agent: Option<String>,
    pub workers: Option<usize>,
    pub timeout: Option<u64>,
    pub no_parallel: bool,
    pub curate: bool,
    pub debug: bool,
    pub output: Option<String>,
    pub fast: bool,
    pub wait: bool,
}

/// Analyze a recording file using an AI agent.
///
/// Reads the cast file, extracts meaningful content, and uses AI to identify
/// key engineering moments. Markers are added directly to the file.
#[cfg(not(tarpaulin_include))]
pub fn handle(args: AnalyzeArgs) -> Result<()> {
    let config = Config::load()?;
    let timeout_secs = args.timeout.or(config.analysis.timeout).unwrap_or(120);
    let agent_name = args
        .agent
        .clone()
        .unwrap_or_else(|| config.resolve_analysis_agent());
    let filepath = checked_session(&args.file, &config)?;
    let service = AnalyzerService::new(options_for(&config, &args, &agent_name)?);
    ensure_agent_available(&service, &agent_name)?;
    prompt_remove_existing_markers(&filepath)?;
    println!("Analyzing {} with {}...", args.file, agent_name);
    let curate = args.curate || config.analysis.curate.unwrap_or(false);
    analyze_and_finish(&service, &filepath, curate, timeout_secs)?;
    if args.wait {
        wait_for_enter()?;
    }
    Ok(())
}

/// Options for the run: CLI > config > defaults, plus per-agent config.
fn options_for(config: &Config, args: &AnalyzeArgs, agent_name: &str) -> Result<AnalyzeOptions> {
    let agent = parse_agent_type(agent_name)?;
    let options = build_analyze_options(
        config,
        agent,
        args.workers,
        args.timeout,
        args.no_parallel,
        args.debug,
        args.output.clone(),
        args.fast,
    );
    Ok(apply_agent_config(
        options,
        config.analysis_agent_config(agent_name),
    ))
}

/// Run the analysis, curate, and offer a rename.
#[cfg(not(tarpaulin_include))]
fn analyze_and_finish(
    service: &AnalyzerService,
    filepath: &Path,
    curate: bool,
    timeout_secs: u64,
) -> Result<()> {
    let result = service.analyze(filepath)?;
    report_markers(&result);
    let (markers, total) = (&result.markers, result.total_duration);
    let count = handle_curation(service, markers, total, curate, filepath, timeout_secs)?;
    println!("\nAnalysis complete. {} markers in file.", count);
    rename::handle_rename_suggestion(service, markers, total, filepath, timeout_secs)
}

/// Resolve the file, refuse corrupt files and files being recorded.
fn checked_session(file: &str, config: &Config) -> Result<std::path::PathBuf> {
    let filepath = super::resolve_existing_session(file, config)?;
    check_file_integrity(&filepath)?;
    agr::files::lock::check_not_locked(&filepath)?;
    Ok(filepath)
}

fn ensure_agent_available(service: &AnalyzerService, agent_name: &str) -> Result<()> {
    if service.is_agent_available() {
        return Ok(());
    }
    anyhow::bail!(
        "Analysis agent '{}' is not installed. Install it or use --agent to specify another.\n\
         Supported agents: claude, codex, gemini",
        agent_name
    )
}

/// Print partial-result warning and the found markers.
fn report_markers(result: &agr::analyzer::AnalysisResult) {
    if result.is_partial() {
        eprintln!(
            "Warning: Analysis partially complete. {} of {} chunks succeeded.",
            result.usage_summary.successful_chunks, result.usage_summary.chunks_processed
        );
    }
    println!("\nMarkers found ({}):", result.markers.len());
    for marker in &result.markers {
        print_marker(marker.timestamp, &marker.label);
    }
}

fn wait_for_enter() -> Result<()> {
    print!("\nPress Enter to continue...");
    io::stdout().flush()?;
    io::stdin().lock().read_line(&mut String::new())?;
    Ok(())
}

/// Build analysis options with three-tier cascade: CLI > config > defaults.
#[allow(clippy::too_many_arguments)]
fn build_analyze_options(
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
fn apply_agent_config(
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

/// Check for existing markers and offer to remove them before analysis.
#[cfg(not(tarpaulin_include))]
fn prompt_remove_existing_markers(filepath: &Path) -> Result<()> {
    let existing_count = MarkerManager::count_markers(filepath)?;
    if existing_count == 0 {
        return Ok(());
    }

    print!(
        "File contains {} existing marker(s). Remove them before analysis? [y/N]: ",
        existing_count
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;

    if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
        let removed = MarkerManager::clear_markers(filepath)?;
        println!("Removed {} marker(s).", removed);
    }

    Ok(())
}

/// Handle the curation flow: prompt if needed, execute, and return final marker count.
#[cfg(not(tarpaulin_include))]
fn handle_curation(
    service: &AnalyzerService,
    markers: &[ValidatedMarker],
    total_duration: f64,
    effective_curate: bool,
    filepath: &Path,
    effective_timeout_secs: u64,
) -> Result<usize> {
    if markers.len() <= CURATION_THRESHOLD {
        return Ok(markers.len());
    }

    let should_curate = prompt_for_curation(markers.len(), effective_curate)?;

    if !should_curate {
        return Ok(markers.len());
    }

    let timeout_duration = Duration::from_secs(effective_timeout_secs);
    match service.curate_markers(markers, total_duration, timeout_duration) {
        Ok(curated) => {
            MarkerManager::clear_markers(filepath)?;
            for marker in &curated {
                MarkerManager::add_marker(filepath, marker.timestamp, &marker.label)?;
            }
            println!("\nCurated markers ({}):", curated.len());
            for marker in &curated {
                print_marker(marker.timestamp, &marker.label);
            }
            Ok(curated.len())
        }
        Err(e) => {
            eprintln!("Warning: Curation failed ({}), keeping all markers.", e);
            Ok(markers.len())
        }
    }
}

/// Prompt the user about curation, or auto-curate if flag is set. Returns true if should curate.
#[cfg(not(tarpaulin_include))]
fn prompt_for_curation(marker_count: usize, effective_curate: bool) -> Result<bool> {
    if effective_curate {
        println!("\nAuto-curating {} markers to 8-12...", marker_count);
        return Ok(true);
    }

    print!(
        "\nFound {} markers. Curate to 8-12 most significant? [y/N]: ",
        marker_count
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    Ok(input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes"))
}

/// Print a marker with formatted timestamp.
fn print_marker(timestamp: f64, label: &str) {
    let minutes = (timestamp / 60.0).floor() as u32;
    let seconds = timestamp % 60.0;
    println!("  {:02}:{:05.2} - {}", minutes, seconds, label);
}

/// Parse agent name string to AgentType enum.
fn parse_agent_type(name: &str) -> Result<AgentType> {
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

mod rename;
#[cfg(test)]
mod tests;
