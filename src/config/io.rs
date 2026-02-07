//! Configuration I/O operations

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::types::Config;

/// Get the config file path (~/.config/agr/config.toml)
pub fn config_path() -> Result<PathBuf> {
    let config_dir = config_dir()?;
    Ok(config_dir.join("config.toml"))
}

/// Get the config directory path (~/.config/agr)
pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".config").join("agr"))
}

/// Load configuration from file, or return defaults if not found
pub fn load() -> Result<Config> {
    let config_path = config_path()?;

    if config_path.exists() {
        let contents = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {:?}", config_path))?;
        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {:?}", config_path))?;
        config
            .analysis
            .validate()
            .map_err(|e| anyhow::anyhow!("Invalid config: {}", e))?;
        Ok(config)
    } else {
        Ok(Config::default())
    }
}

/// Save configuration to file
pub fn save(config: &Config) -> Result<()> {
    let config_path = config_path()?;

    // Ensure config directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {:?}", parent))?;
    }

    let contents = toml::to_string_pretty(config).context("Failed to serialize config")?;
    fs::write(&config_path, contents)
        .with_context(|| format!("Failed to write config file: {:?}", config_path))?;

    Ok(())
}
