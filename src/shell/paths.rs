//! Path detection for shell integration
//!
//! This module handles detecting RC files and completion directories.

use std::path::PathBuf;

/// Get the path to the default shell RC file
pub fn detect_shell_rc() -> Option<PathBuf> {
    let home = dirs::home_dir()?;

    // Check zshrc first (more common on macOS)
    let zshrc = home.join(".zshrc");
    if zshrc.exists() {
        return Some(zshrc);
    }

    // Check bashrc
    let bashrc = home.join(".bashrc");
    if bashrc.exists() {
        return Some(bashrc);
    }

    // Default to zshrc if neither exists
    Some(zshrc)
}

/// Get all possible shell RC files
pub fn all_shell_rcs() -> Vec<PathBuf> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return vec![],
    };

    vec![home.join(".zshrc"), home.join(".bashrc")]
}

/// Get the default bash completion installation path
pub fn bash_completion_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(
        home.join(".local")
            .join("share")
            .join("bash-completion")
            .join("completions")
            .join("agr"),
    )
}

/// Get the default zsh completion installation path
pub fn zsh_completion_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".zsh").join("completions").join("_agr"))
}

/// Get the default script path (in the config directory)
pub fn default_script_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".config").join("agr").join("agr.sh"))
}
