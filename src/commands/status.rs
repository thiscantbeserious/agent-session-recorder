//! Status command handler

use anyhow::Result;

use agr::tui::current_theme;
use agr::{Config, StorageManager};

/// Display storage statistics for recorded sessions.
///
/// Shows total size, disk usage percentage, session count by agent,
/// and age of the oldest recording.
#[cfg(not(tarpaulin_include))]
pub fn handle() -> Result<()> {
    let config = Config::load()?;
    let storage = StorageManager::new(config);
    let stats = storage.get_stats()?;
    let theme = current_theme();
    println!("{}", theme.primary_text(&stats.summary()));
    Ok(())
}
