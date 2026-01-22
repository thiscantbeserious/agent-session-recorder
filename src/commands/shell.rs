//! Shell subcommands handler

use anyhow::Result;

use agr::Config;

/// Show shell integration status.
#[cfg(not(tarpaulin_include))]
pub fn handle_status() -> Result<()> {
    let config = Config::load()?;
    let status = agr::shell::get_status(config.shell.auto_wrap);
    println!("{}", status.summary());
    Ok(())
}

/// Install shell integration to .zshrc/.bashrc.
///
/// Creates wrapper functions for configured agents that automatically record sessions.
/// The shell script is embedded directly in the RC file (not sourced from an external file).
#[cfg(not(tarpaulin_include))]
pub fn handle_install() -> Result<()> {
    // Create config.toml with defaults if it doesn't exist
    let config_path = Config::config_path()?;
    if !config_path.exists() {
        let config = Config::default();
        config.save()?;
        println!("Created config file: {}", config_path.display());
    }

    // Detect shell RC file
    let rc_file = agr::shell::detect_shell_rc()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    // Install shell integration to RC file (script is embedded directly)
    agr::shell::install(&rc_file)
        .map_err(|e| anyhow::anyhow!("Failed to install shell integration: {}", e))?;
    println!("Installed shell integration: {}", rc_file.display());

    // Install completions
    install_completions()?;

    println!();
    println!("Shell integration installed successfully.");
    println!("Restart your shell or run: source {}", rc_file.display());

    Ok(())
}

/// Install shell completions for bash and zsh.
pub(crate) fn install_completions() -> Result<()> {
    if let Some(path) = agr::shell::install_bash_completions()
        .map_err(|e| anyhow::anyhow!("Failed to install bash completions: {}", e))?
    {
        println!("Installed bash completions: {}", path.display());
    }
    if let Some(path) = agr::shell::install_zsh_completions()
        .map_err(|e| anyhow::anyhow!("Failed to install zsh completions: {}", e))?
    {
        println!("Installed zsh completions: {}", path.display());
    }
    Ok(())
}

/// Remove shell integration from .zshrc/.bashrc.
#[cfg(not(tarpaulin_include))]
pub fn handle_uninstall() -> Result<()> {
    // Find where shell integration is installed
    let rc_file = match agr::shell::find_installed_rc() {
        Some(rc) => rc,
        None => {
            println!("Shell integration is not installed.");
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
        println!("Removed shell integration from: {}", rc_file.display());

        // Clean up old-style external script file if it exists
        if let Some(script_path) = old_script_path {
            if script_path.exists() {
                std::fs::remove_file(&script_path)
                    .map_err(|e| anyhow::anyhow!("Failed to remove shell script: {}", e))?;
                println!("Removed old shell script: {}", script_path.display());
            }
        }

        remove_completions()?;

        println!();
        println!("Shell integration removed successfully.");
        println!("Restart your shell to complete the removal.");
    } else {
        println!("Shell integration was not found in: {}", rc_file.display());
    }

    Ok(())
}

/// Remove shell completions for bash and zsh.
pub(crate) fn remove_completions() -> Result<()> {
    if agr::shell::uninstall_bash_completions()
        .map_err(|e| anyhow::anyhow!("Failed to remove bash completions: {}", e))?
    {
        if let Some(path) = agr::shell::bash_completion_path() {
            println!("Removed bash completions: {}", path.display());
        }
    }
    if agr::shell::uninstall_zsh_completions()
        .map_err(|e| anyhow::anyhow!("Failed to remove zsh completions: {}", e))?
    {
        if let Some(path) = agr::shell::zsh_completion_path() {
            println!("Removed zsh completions: {}", path.display());
        }
    }
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
