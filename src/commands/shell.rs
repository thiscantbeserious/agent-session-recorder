//! Shell subcommands handler

use anyhow::Result;

use agr::theme::current_theme;
use agr::Config;

/// Show shell integration status.
#[cfg(not(tarpaulin_include))]
pub fn handle_status() -> Result<()> {
    let config = Config::load()?;
    let theme = current_theme();
    let status = agr::shell::get_status(config.shell.auto_wrap);
    println!("{}", theme.primary_text(&status.summary()));
    Ok(())
}

/// Install shell integration to .zshrc/.bashrc.
///
/// Creates wrapper functions for configured agents that automatically record sessions.
/// The shell script is embedded directly in the RC file (not sourced from an external file).
#[cfg(not(tarpaulin_include))]
pub fn handle_install() -> Result<()> {
    let theme = current_theme();
    // Create config.toml with defaults if it doesn't exist
    let config_path = Config::config_path()?;
    if !config_path.exists() {
        let config = Config::default();
        config.save()?;
        println!(
            "{}",
            theme.primary_text(&format!("Created config file: {}", config_path.display()))
        );
    }

    // Detect shell RC file
    let rc_file = agr::shell::detect_shell_rc()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    // Install shell integration to RC file (script is embedded directly)
    agr::shell::install(&rc_file)
        .map_err(|e| anyhow::anyhow!("Failed to install shell integration: {}", e))?;
    println!(
        "{}",
        theme.primary_text(&format!(
            "Installed shell integration: {}",
            rc_file.display()
        ))
    );

    // Install completions
    install_completions()?;

    println!();
    println!(
        "{}",
        theme.primary_text("Shell integration installed successfully.")
    );
    println!(
        "{}",
        theme.primary_text(&format!(
            "Restart your shell or run: source {}",
            rc_file.display()
        ))
    );

    Ok(())
}

/// Clean up old static completion files from previous installations.
///
/// Completions are now embedded in the RC file section (generated dynamically),
/// so old static completion files are no longer needed.
pub(crate) fn install_completions() -> Result<()> {
    // Clean up old completion files - completions are now embedded in RC file
    agr::shell::cleanup_old_completions()
        .map_err(|e| anyhow::anyhow!("Failed to clean up old completions: {}", e))?;
    Ok(())
}

/// Remove shell integration from .zshrc/.bashrc.
#[cfg(not(tarpaulin_include))]
pub fn handle_uninstall() -> Result<()> {
    let theme = current_theme();
    // Find where shell integration is installed
    let rc_file = match agr::shell::find_installed_rc() {
        Some(rc) => rc,
        None => {
            println!(
                "{}",
                theme.primary_text("Shell integration is not installed.")
            );
            return Ok(());
        }
    };

    // Check for old-style installation (external script file) before removing
    // so we can clean it up for backward compatibility
    let old_script_path = agr::shell::extract_script_path(&rc_file)
        .ok()
        .flatten()
        .or_else(agr::shell::default_script_path);

    // Remove from RC file
    let removed = agr::shell::uninstall(&rc_file)
        .map_err(|e| anyhow::anyhow!("Failed to remove shell integration: {}", e))?;

    if removed {
        println!(
            "{}",
            theme.primary_text(&format!(
                "Removed shell integration from: {}",
                rc_file.display()
            ))
        );

        // Clean up old-style external script file if it exists
        if let Some(script_path) = old_script_path {
            if script_path.exists() {
                std::fs::remove_file(&script_path)
                    .map_err(|e| anyhow::anyhow!("Failed to remove shell script: {}", e))?;
                println!(
                    "{}",
                    theme.primary_text(&format!(
                        "Removed old shell script: {}",
                        script_path.display()
                    ))
                );
            }
        }

        remove_completions()?;

        println!();
        println!(
            "{}",
            theme.primary_text("Shell integration removed successfully.")
        );
        println!(
            "{}",
            theme.primary_text("Restart your shell to complete the removal.")
        );
    } else {
        println!(
            "{}",
            theme.primary_text(&format!(
                "Shell integration was not found in: {}",
                rc_file.display()
            ))
        );
    }

    Ok(())
}

/// Remove any leftover static completion files.
///
/// Completions are now embedded in the RC file section (generated dynamically),
/// so old static completion files should be removed during uninstall.
pub(crate) fn remove_completions() -> Result<()> {
    // Clean up old completion files - this is the same as install
    // since completions are now embedded in RC file
    agr::shell::cleanup_old_completions()
        .map_err(|e| anyhow::anyhow!("Failed to clean up old completions: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_completions_runs_without_error() {
        // This test may actually install completions, but should not panic
        // The underlying functions handle missing directories gracefully
        let result = install_completions();
        // On systems where completion dirs exist, this will succeed
        // On systems without them, it should still not panic
        assert!(result.is_ok());
    }

    #[test]
    fn remove_completions_runs_without_error() {
        // Should not panic even if completion files don't exist
        let result = remove_completions();
        assert!(result.is_ok());
    }
}
