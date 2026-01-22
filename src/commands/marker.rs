//! Marker subcommands handler

use anyhow::Result;

use agr::{Config, MarkerManager};

use super::resolve_file_path;

/// Add a marker to a cast file at a specific timestamp.
///
/// Markers use the native asciicast v3 marker format.
#[cfg(not(tarpaulin_include))]
pub fn handle_add(file: &str, time: f64, label: &str) -> Result<()> {
    let config = Config::load()?;
    // Resolve file path (supports short format like "claude/session.cast")
    let filepath = resolve_file_path(file, &config)?;
    MarkerManager::add_marker(&filepath, time, label)?;
    println!("Marker added at {:.1}s: \"{}\"", time, label);
    Ok(())
}

/// List all markers in a cast file with their timestamps and labels.
#[cfg(not(tarpaulin_include))]
pub fn handle_list(file: &str) -> Result<()> {
    let config = Config::load()?;
    // Resolve file path (supports short format like "claude/session.cast")
    let filepath = resolve_file_path(file, &config)?;
    let markers = MarkerManager::list_markers(&filepath)?;

    if markers.is_empty() {
        println!("No markers found in file.");
        return Ok(());
    }

    println!("Markers:");
    for marker in markers {
        println!("  {}", marker);
    }

    Ok(())
}
