//! Status command handler

use anyhow::Result;

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
    println!("{}", stats.summary());
    Ok(())
}
