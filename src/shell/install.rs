//! RC file installation and uninstallation
//!
//! This module handles installing and uninstalling the shell integration
//! section in RC files (.zshrc, .bashrc).

use std::fs;
use std::io;
use std::path::Path;

use super::completions::{generate_bash_init, generate_zsh_init};
use super::minify;
use super::status::{is_installed_in, MARKER_END, MARKER_START};

/// Warning comment included in the shell integration section
pub const MARKER_WARNING: &str = "# DO NOT EDIT - managed by 'agr shell install/uninstall'";

/// The embedded shell script content (wrapper functions for agents)
pub const SHELL_SCRIPT: &str = include_str!("../../shell/agr.sh");

/// Shell type for completion generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
}

/// Detect shell type from RC file name
fn detect_shell_from_rc(rc_file: &Path) -> Shell {
    let name = rc_file.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if name.contains("zsh") {
        Shell::Zsh
    } else {
        Shell::Bash
    }
}

/// Generate the shell integration section content
///
/// This embeds the full shell script content directly into the RC file
/// instead of sourcing an external file. This ensures shell wrappers
/// survive shell snapshots (e.g., Claude Code's shell-snapshots mechanism).
///
/// The section now includes:
/// - Agent wrapper functions (from agr.sh)
/// - Dynamic completions generated from clap command definitions
/// - Ghost text autosuggestions for file-accepting commands
///
/// The combined script is minified to reduce the size of the RC file.
pub fn generate_section(shell: Shell) -> String {
    // Generate shell-specific init code with embedded completions
    // Use debug=false for minified output in RC files
    let init_code = match shell {
        Shell::Zsh => generate_zsh_init(false),
        Shell::Bash => generate_bash_init(false),
    };

    // Combine wrapper script with completions and minify
    let combined = format!("{}\n{}", SHELL_SCRIPT, init_code);
    let minified = minify::exec(&combined);

    format!("{MARKER_START}\n{MARKER_WARNING}\n{minified}\n{MARKER_END}")
}

/// Install shell integration to an RC file
///
/// This embeds the full shell script content directly into the RC file,
/// including dynamically generated completions based on shell type.
/// If there's an existing installation (old-style or new), it will be replaced.
pub fn install(rc_file: &Path) -> io::Result<()> {
    // Detect shell type from RC file
    let shell = detect_shell_from_rc(rc_file);

    // First, remove any existing installation (handles both old and new style)
    if is_installed_in(rc_file)? {
        uninstall(rc_file)?;
    }

    // Read existing content
    let content = if rc_file.exists() {
        fs::read_to_string(rc_file)?
    } else {
        String::new()
    };

    // Generate section with embedded script and completions
    let section = generate_section(shell);

    // Append to file
    let new_content = if content.is_empty() {
        section
    } else if content.ends_with('\n') {
        format!("{content}\n{section}\n")
    } else {
        format!("{content}\n\n{section}\n")
    };

    fs::write(rc_file, new_content)
}

/// Uninstall shell integration from an RC file
pub fn uninstall(rc_file: &Path) -> io::Result<bool> {
    if !rc_file.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(rc_file)?;

    if !content.contains(MARKER_START) {
        return Ok(false);
    }

    // Remove the marked section
    let mut new_lines: Vec<&str> = Vec::new();
    let mut in_section = false;

    for line in content.lines() {
        if line.contains(MARKER_START) {
            in_section = true;
            continue;
        }
        if line.contains(MARKER_END) {
            in_section = false;
            continue;
        }
        if !in_section {
            new_lines.push(line);
        }
    }

    // Remove trailing empty lines that were before the section
    while new_lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        new_lines.pop();
    }

    let new_content = if new_lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", new_lines.join("\n"))
    };

    fs::write(rc_file, new_content)?;
    Ok(true)
}

/// Install the shell script to the config directory
pub fn install_script(script_path: &Path) -> io::Result<()> {
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(script_path, SHELL_SCRIPT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_shell_zsh() {
        assert_eq!(detect_shell_from_rc(&PathBuf::from(".zshrc")), Shell::Zsh);
        assert_eq!(
            detect_shell_from_rc(&PathBuf::from("/home/user/.zshrc")),
            Shell::Zsh
        );
    }

    #[test]
    fn detect_shell_bash() {
        assert_eq!(detect_shell_from_rc(&PathBuf::from(".bashrc")), Shell::Bash);
        assert_eq!(
            detect_shell_from_rc(&PathBuf::from("/home/user/.bashrc")),
            Shell::Bash
        );
        // Default to bash for unknown
        assert_eq!(
            detect_shell_from_rc(&PathBuf::from(".profile")),
            Shell::Bash
        );
    }

    #[test]
    fn generate_section_zsh_contains_markers() {
        let section = generate_section(Shell::Zsh);
        assert!(section.contains(MARKER_START));
        assert!(section.contains(MARKER_END));
        assert!(section.contains(MARKER_WARNING));
    }

    #[test]
    fn generate_section_bash_contains_markers() {
        let section = generate_section(Shell::Bash);
        assert!(section.contains(MARKER_START));
        assert!(section.contains(MARKER_END));
        assert!(section.contains(MARKER_WARNING));
    }

    #[test]
    fn generate_section_contains_completions() {
        let zsh_section = generate_section(Shell::Zsh);
        // Zsh section should contain zsh-specific completion code
        assert!(zsh_section.contains("_agr_complete"));
        assert!(zsh_section.contains("compdef"));

        let bash_section = generate_section(Shell::Bash);
        // Bash section should contain bash-specific completion code
        assert!(bash_section.contains("_agr_complete"));
        assert!(bash_section.contains("complete -F"));
    }

    #[test]
    fn generate_section_contains_wrapper_code() {
        let section = generate_section(Shell::Zsh);
        // Should contain wrapper setup function
        assert!(section.contains("_agr_setup_wrappers"));
    }
}
