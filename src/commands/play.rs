//! Play command handler

use anyhow::Result;

use agr::{play_session, Config};

use super::resolve_file_path;

/// Play a recording file using the native player.
///
/// Resolves the file path and invokes the native player for playback.
/// Supports absolute paths, short format (agent/file.cast), and fuzzy matching.
#[cfg(not(tarpaulin_include))]
pub fn handle(file: &str) -> Result<()> {
    let config = Config::load()?;

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

    // Play the session using the native player
    let result = play_session(&filepath)?;
    println!("{}", result.message());
    Ok(())
}
