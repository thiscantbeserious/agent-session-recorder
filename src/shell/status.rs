//! Status detection and reporting for shell integration
//!
//! This module handles checking installation status and extracting
//! information from installed shell integrations.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::paths::all_shell_rcs;

/// Marker comments for shell integration sections
pub const MARKER_START: &str = "# >>> AGR (Agent Session Recorder) >>>";
pub const MARKER_END: &str = "# <<< AGR (Agent Session Recorder) <<<";

/// Information about shell integration status
#[derive(Debug, Clone)]
pub struct ShellStatus {
    /// Which RC file has the integration installed
    pub rc_file: Option<PathBuf>,
    /// Path to the shell script being sourced
    pub script_path: Option<PathBuf>,
    /// Whether auto_wrap is enabled in config
    pub auto_wrap_enabled: bool,
    /// Whether the integration is currently active (sourced in current shell)
    pub is_active: bool,
}

impl ShellStatus {
    /// Returns a human-readable summary of the status
    pub fn summary(&self) -> String {
        let mut lines = Vec::new();

        if let Some(ref rc) = self.rc_file {
            lines.push(format!("Shell integration: installed in {}", rc.display()));
        } else {
            lines.push("Shell integration: not installed".to_string());
        }

        if let Some(ref script) = self.script_path {
            lines.push(format!("Shell script: {}", script.display()));
        }

        lines.push(format!(
            "Auto-wrap: {}",
            if self.auto_wrap_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ));

        if self.is_active {
            lines.push("Status: active (shell functions loaded)".to_string());
        } else if self.rc_file.is_some() {
            lines.push("Status: installed (restart shell to activate)".to_string());
        }

        lines.join("\n")
    }
}

/// Check if shell integration is installed in an RC file
pub fn is_installed_in(rc_file: &Path) -> io::Result<bool> {
    if !rc_file.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(rc_file)?;
    Ok(content.contains(MARKER_START) && content.contains(MARKER_END))
}

/// Find which RC file has shell integration installed
pub fn find_installed_rc() -> Option<PathBuf> {
    all_shell_rcs()
        .into_iter()
        .find(|rc| is_installed_in(rc).unwrap_or(false))
}

/// Extract the script path from an installed RC file (for old-style installations)
///
/// This function is used for backward compatibility to detect old-style installations
/// where the script was sourced from an external file. New installations embed the
/// script directly and will return None.
pub fn extract_script_path(rc_file: &Path) -> io::Result<Option<PathBuf>> {
    if !rc_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(rc_file)?;

    // Look for old-style source line between markers
    // The old pattern is: [ -f "/path/to/agr.sh" ] && source "/path/to/agr.sh"
    // This should be at the start of a line (not indented, unlike internal script code)
    let in_section = content
        .lines()
        .skip_while(|line| !line.contains(MARKER_START))
        .take_while(|line| !line.contains(MARKER_END))
        .find(|line| {
            // Old-style: starts with [ -f and contains source
            // Check the raw line start (not trimmed) so indented internal script lines don't match
            line.starts_with("[ -f \"") && line.contains("&& source")
        });

    if let Some(line) = in_section {
        // Extract path from: [ -f "/path/to/agr.sh" ] && source "/path/to/agr.sh"
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                let path = &line[start + 1..start + 1 + end];
                return Ok(Some(PathBuf::from(path)));
            }
        }
    }

    Ok(None)
}

/// Get the shell integration status
pub fn get_status(auto_wrap_enabled: bool) -> ShellStatus {
    let rc_file = find_installed_rc();
    let script_path = rc_file
        .as_ref()
        .and_then(|rc| extract_script_path(rc).ok().flatten());

    // Check if integration is active by looking for AGR env var
    let is_active = std::env::var("_AGR_LOADED").is_ok();

    ShellStatus {
        rc_file,
        script_path,
        auto_wrap_enabled,
        is_active,
    }
}
