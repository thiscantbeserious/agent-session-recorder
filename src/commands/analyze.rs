//! Analyze command handler

use anyhow::Result;

use agr::{Analyzer, Config};

use super::resolve_file_path;

/// Analyze a recording file using an AI agent.
///
/// Reads the cast file and uses the specified (or configured) analysis agent
/// to identify key moments and add markers.
#[cfg(not(tarpaulin_include))]
pub fn handle(file: &str, agent_override: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let agent = agent_override.unwrap_or(&config.recording.analysis_agent);

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

    // Check agent is installed
    if !Analyzer::is_agent_installed(agent) {
        anyhow::bail!(
            "Analysis agent '{}' is not installed. Install it or use --agent to specify another.",
            agent
        );
    }

    println!("Analyzing {} with {}...", file, agent);
    let analyzer = Analyzer::new(agent);
    analyzer.analyze(&filepath)?;
    println!("Analysis complete.");
    Ok(())
}
