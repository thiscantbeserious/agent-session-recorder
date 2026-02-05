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

use std::io::{self, Write};

use anyhow::Result;

use agr::analyzer::{AgentType, AnalyzeOptions, AnalyzerService};
use agr::{Config, MarkerManager};

use super::resolve_file_path;

/// Analyze a recording file using an AI agent.
///
/// Reads the cast file, extracts meaningful content, and uses AI to identify
/// key engineering moments. Markers are added directly to the file.
#[cfg(not(tarpaulin_include))]
pub fn handle(
    file: &str,
    agent_override: Option<&str>,
    workers: Option<usize>,
    timeout: Option<u64>,
    no_parallel: bool,
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
    println!("\nMarkers added ({}):", result.markers_added());
    for marker in &result.markers {
        let minutes = (marker.timestamp / 60.0).floor() as u32;
        let seconds = marker.timestamp % 60.0;
        println!(
            "  {:02}:{:05.2} - {}",
            minutes, seconds, marker.label
        );
    }

    println!("\nAnalysis complete.");

    Ok(())
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
