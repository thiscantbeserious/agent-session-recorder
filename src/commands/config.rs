//! Config subcommands handler

use anyhow::Result;

use agr::Config;

/// Show current configuration as TOML.
#[cfg(not(tarpaulin_include))]
pub fn handle_show() -> Result<()> {
    let config = Config::load()?;
    let toml_str = toml::to_string_pretty(&config)?;
    println!("{}", toml_str);
    Ok(())
}

/// Open configuration file in the default editor.
///
/// Uses $EDITOR environment variable (defaults to 'vi').
#[cfg(not(tarpaulin_include))]
pub fn handle_edit() -> Result<()> {
    let config_path = Config::config_path()?;

    // Ensure config exists
    if !config_path.exists() {
        let config = Config::default();
        config.save()?;
    }

    // Get editor from environment
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    println!("Opening {} with {}", config_path.display(), editor);

    std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))?;

    Ok(())
}
