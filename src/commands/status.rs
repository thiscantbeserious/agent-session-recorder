//! Status command handler

use anyhow::Result;

use super::compress_offer::savings_line;
use agr::storage::compress;
use agr::theme::current_theme;
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
    if let Some(line) = savings_line(&compress::plan(&storage, None)?) {
        println!("{}", theme.primary_text(&line));
    }
    let found = agr::storage::orphans::find(&storage.storage_dir())?;
    if let Some(line) = super::cleanup::orphans::status_line(&found) {
        println!("{}", theme.primary_text(&line));
    }
    Ok(())
}
