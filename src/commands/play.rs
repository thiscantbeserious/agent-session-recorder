//! Play command handler

use anyhow::Result;

use agr::{play_session, Config};

use agr::asciicast::integrity::check_file_integrity;

/// Play a recording file using the native player.
///
/// Resolves the file path and invokes the native player for playback.
/// Supports absolute paths, short format (agent/file.cast), and fuzzy matching.
#[cfg(not(tarpaulin_include))]
pub fn handle(file: &str) -> Result<()> {
    let config = Config::load()?;

    let filepath = super::resolve_existing_session(file, &config)?;

    // Check for file corruption before playing
    check_file_integrity(&filepath)?;

    // Play the session using the native player
    let result = play_session(&filepath)?;
    println!("{}", result.message());
    Ok(())
}
