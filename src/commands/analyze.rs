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
use std::time::Duration;

use anyhow::Result;

use agr::analyzer::{AgentType, AnalyzeOptions, AnalyzerService};
use agr::{Config, MarkerManager};

use agr::asciicast::integrity::check_file_integrity;
use agr::files::resolve::resolve_file_path;

/// Threshold for offering marker curation.
const CURATION_THRESHOLD: usize = 12;

/// Analyze a recording file using an AI agent.
///
/// Reads the cast file, extracts meaningful content, and uses AI to identify
/// key engineering moments. Markers are added directly to the file.
#[cfg(not(tarpaulin_include))]
#[allow(clippy::too_many_arguments)]
pub fn handle(
    file: &str,
    agent_override: Option<&str>,
    workers: Option<usize>,
    timeout: Option<u64>,
    no_parallel: bool,
    curate: bool,
    debug: bool,
    output: Option<String>,
    fast: bool,
    wait: bool,
) -> Result<()> {
    let config = Config::load()?;

    // Resolve agent: CLI override > config > default
    let resolved_agent = match agent_override {
        Some(name) => name.to_string(),
        None => config.resolve_analysis_agent(),
    };
    let agent = parse_agent_type(&resolved_agent)?;

    // Resolve file path (supports short format like "claude/session.cast")
    let filepath = resolve_file_path(file, &config)?;
    if !filepath.exists() {
        anyhow::bail!(
            "File not found: {}\nHint: Use format 'agent/file.cast'. Run 'agr list' to see available sessions.",
            file
        );
    }

    // Check file has .cast extension
    if filepath.extension().and_then(|e| e.to_str()) != Some("cast") {
        eprintln!("Warning: File does not have .cast extension");
    }

    // Check for file corruption before proceeding
    check_file_integrity(&filepath)?;

    // Refuse to analyze a file being actively recorded
    agr::files::lock::check_not_locked(&filepath)?;

    // Look up per-agent config
    let agent_config = config.analysis_agent_config(&resolved_agent);

    // Build options with three-tier cascade: CLI > config > defaults
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

    // Pass per-task extra_args and token_budget_override from per-agent config
    if let Some(ac) = agent_config {
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
    }

    // Create service
    let service = AnalyzerService::new(options);
    let agent_name = &resolved_agent;

    // Check agent is available
    if !service.is_agent_available() {
        anyhow::bail!(
            "Analysis agent '{}' is not installed. Install it or use --agent to specify another.\n\
             Supported agents: claude, codex, gemini",
            agent_name
        );
    }

    // Check for existing markers and offer to remove them
    let existing_count = MarkerManager::count_markers(&filepath)?;
    if existing_count > 0 {
        print!(
            "File contains {} existing marker(s). Remove them before analysis? [y/N]: ",
            existing_count
        );
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes") {
            let removed = MarkerManager::clear_markers(&filepath)?;
            println!("Removed {} marker(s).", removed);
        }
    }

    // Run analysis
    println!("Analyzing {} with {}...", file, agent);
    let result = service.analyze(&filepath)?;

    // Report results
    if result.is_partial() {
        eprintln!(
            "Warning: Analysis partially complete. {} of {} chunks succeeded.",
            result.usage_summary.successful_chunks, result.usage_summary.chunks_processed
        );
    }

    // Print markers verbosely
    println!("\nMarkers found ({}):", result.markers.len());
    for marker in &result.markers {
        print_marker(marker.timestamp, &marker.label);
    }

    // Handle curation if we have many markers
    // Curate: CLI true wins, else config, else false
    let effective_curate = curate || config.analysis.curate.unwrap_or(false);
    let final_marker_count = if result.markers.len() > CURATION_THRESHOLD {
        let should_curate = if effective_curate {
            // Auto-curate with --curate flag
            println!(
                "\nAuto-curating {} markers to 8-12...",
                result.markers.len()
            );
            true
        } else {
            // Prompt user
            print!(
                "\nFound {} markers. Curate to 8-12 most significant? [y/N]: ",
                result.markers.len()
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().lock().read_line(&mut input)?;
            input.trim().eq_ignore_ascii_case("y") || input.trim().eq_ignore_ascii_case("yes")
        };

        if should_curate {
            let timeout_duration = Duration::from_secs(timeout.unwrap_or(120));
            match service.curate_markers(&result.markers, result.total_duration, timeout_duration) {
                Ok(curated) => {
                    // Write curated markers to file (replacing the ones from analyze)
                    MarkerManager::clear_markers(&filepath)?;
                    for marker in &curated {
                        MarkerManager::add_marker(&filepath, marker.timestamp, &marker.label)?;
                    }

                    println!("\nCurated markers ({}):", curated.len());
                    for marker in &curated {
                        print_marker(marker.timestamp, &marker.label);
                    }
                    curated.len()
                }
                Err(e) => {
                    eprintln!("Warning: Curation failed ({}), keeping all markers.", e);
                    result.markers.len()
                }
            }
        } else {
            result.markers.len()
        }
    } else {
        result.markers.len()
    };

    println!(
        "\nAnalysis complete. {} markers in file.",
        final_marker_count
    );

    // Suggest a descriptive filename via LLM
    if !result.markers.is_empty() {
        let current_filename = filepath
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let timeout_duration = Duration::from_secs(timeout.unwrap_or(120));

        match service.suggest_rename(
            &result.markers,
            result.total_duration,
            timeout_duration,
            &current_filename,
        ) {
            Some(suggested) => {
                let suggested_file = format!("{}.cast", suggested);
                let new_path = filepath.with_file_name(&suggested_file);
                if new_path != filepath && !new_path.exists() {
                    print!("\nRename to \"{}\"? [y/N]: ", suggested_file);
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().lock().read_line(&mut input)?;

                    if input.trim().eq_ignore_ascii_case("y")
                        || input.trim().eq_ignore_ascii_case("yes")
                    {
                        std::fs::rename(&filepath, &new_path)?;
                        println!("Renamed to: {}", new_path.display());
                    }
                }
            }
            None => {
                // Silently skip if rename suggestion fails
            }
        }
    }

    if wait {
        print!("\nPress Enter to continue...");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().lock().read_line(&mut input)?;
    }

    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
