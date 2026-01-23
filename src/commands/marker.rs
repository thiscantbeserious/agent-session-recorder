//! Marker subcommands handler

use anyhow::Result;

use agr::tui::current_theme;
use agr::{Config, MarkerManager};

use super::resolve_file_path;

/// Add a marker to a cast file at a specific timestamp.
///
/// Markers use the native asciicast v3 marker format.
#[cfg(not(tarpaulin_include))]
pub fn handle_add(file: &str, time: f64, label: &str) -> Result<()> {
    let config = Config::load()?;
    let theme = current_theme();
    // Resolve file path (supports short format like "claude/session.cast")
    let filepath = resolve_file_path(file, &config)?;
    MarkerManager::add_marker(&filepath, time, label)?;
    println!(
        "{}",
        theme.primary_text(&format!("Marker added at {:.1}s: \"{}\"", time, label))
    );
    Ok(())
}

/// List all markers in a cast file with their timestamps and labels.
#[cfg(not(tarpaulin_include))]
pub fn handle_list(file: &str) -> Result<()> {
    let config = Config::load()?;
    let theme = current_theme();
    // Resolve file path (supports short format like "claude/session.cast")
    let filepath = resolve_file_path(file, &config)?;
    let markers = MarkerManager::list_markers(&filepath)?;

    if markers.is_empty() {
        println!("{}", theme.primary_text("No markers found in file."));
        return Ok(());
    }

    println!("{}", theme.primary_text("Markers:"));
    for marker in markers {
        println!("{}", theme.primary_text(&format!("  {}", marker)));
    }

    Ok(())
}
