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

use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;

use agr::analyzer::{AgentType, AnalyzeOptions, AnalyzerService};
use agr::{Config, MarkerManager};

use super::resolve_file_path;

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
) -> Result<()> {
    let config = Config::load()?;
    let agent_name = agent_override.unwrap_or(&config.recording.analysis_agent);

    // Parse agent type
    let agent = parse_agent_type(agent_name)?;

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

    // Build options
    let mut options = AnalyzeOptions::with_agent(agent);
    if let Some(w) = workers {
        options = options.workers(w);
    }
    if let Some(t) = timeout {
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

    // Create service
    let service = AnalyzerService::new(options);

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
        io::stdin().read_line(&mut input)?;

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
    let final_marker_count = if result.markers.len() > CURATION_THRESHOLD {
        let should_curate = if curate {
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
            io::stdin().read_line(&mut input)?;
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
        "gemini" | "gemini-cli" => Ok(AgentType::Gemini),
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
        assert_eq!(parse_agent_type("gemini-cli").unwrap(), AgentType::Gemini);
    }

    #[test]
    fn parse_agent_type_unknown() {
        assert!(parse_agent_type("unknown").is_err());
    }
}
