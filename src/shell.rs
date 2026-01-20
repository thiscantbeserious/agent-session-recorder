//! Shell integration management for AGR
//!
//! This module handles installing and uninstalling shell integration
//! to .zshrc and .bashrc files using marked sections.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Marker comments for shell integration sections
const MARKER_START: &str = "# >>> AGR (Agent Session Recorder) >>>";
const MARKER_END: &str = "# <<< AGR (Agent Session Recorder) <<<";
const MARKER_WARNING: &str = "# DO NOT EDIT - managed by 'agr shell install/uninstall'";

/// The embedded shell script content
pub const SHELL_SCRIPT: &str = include_str!("../shell/agr.sh");

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

/// Extract the script path from an installed RC file
pub fn extract_script_path(rc_file: &Path) -> io::Result<Option<PathBuf>> {
    if !rc_file.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(rc_file)?;

    // Look for source line between markers
    let in_section = content
        .lines()
        .skip_while(|line| !line.contains(MARKER_START))
        .take_while(|line| !line.contains(MARKER_END))
        .find(|line| line.contains("source") || line.contains(". "));

    if let Some(line) = in_section {
        // Extract path from: [ -f "/path/to/agr.sh" ] && source "/path/to/agr.sh"
        // or: source "/path/to/agr.sh"
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                let path = &line[start + 1..start + 1 + end];
                return Ok(Some(PathBuf::from(path)));
            }
        }
    }

    Ok(None)
}

/// Generate the shell integration section content
pub fn generate_section(script_path: &Path) -> String {
    let script_path_str = script_path.display();
    format!(
        r#"{MARKER_START}
{MARKER_WARNING}
[ -f "{script_path_str}" ] && source "{script_path_str}"
{MARKER_END}"#
    )
}

/// Install shell integration to an RC file
pub fn install(rc_file: &Path, script_path: &Path) -> io::Result<()> {
    // First, remove any existing installation
    if is_installed_in(rc_file)? {
        uninstall(rc_file)?;
    }

    // Read existing content
    let content = if rc_file.exists() {
        fs::read_to_string(rc_file)?
    } else {
        String::new()
    };

    // Generate section
    let section = generate_section(script_path);

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

/// Get the default script path (in the config directory)
pub fn default_script_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".config").join("agr").join("agr.sh"))
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
    use tempfile::TempDir;

    #[test]
    fn test_generate_section_contains_markers() {
        let script_path = PathBuf::from("/path/to/agr.sh");
        let section = generate_section(&script_path);

        assert!(section.contains(MARKER_START));
        assert!(section.contains(MARKER_END));
        assert!(section.contains(MARKER_WARNING));
        assert!(section.contains("/path/to/agr.sh"));
    }

    #[test]
    fn test_install_creates_section_in_rc() -> io::Result<()> {
        let temp = TempDir::new()?;
        let rc_file = temp.path().join(".zshrc");
        let script_path = PathBuf::from("/path/to/agr.sh");

        // Create empty RC file
        fs::write(&rc_file, "")?;

        // Install
        install(&rc_file, &script_path)?;

        // Verify
        let content = fs::read_to_string(&rc_file)?;
        assert!(content.contains(MARKER_START));
        assert!(content.contains(MARKER_END));
        assert!(content.contains("/path/to/agr.sh"));

        Ok(())
    }

    #[test]
    fn test_install_appends_to_existing_content() -> io::Result<()> {
        let temp = TempDir::new()?;
        let rc_file = temp.path().join(".zshrc");
        let script_path = PathBuf::from("/path/to/agr.sh");

        // Create RC file with existing content
        fs::write(&rc_file, "# My shell config\nexport FOO=bar\n")?;

        // Install
        install(&rc_file, &script_path)?;

        // Verify existing content preserved
        let content = fs::read_to_string(&rc_file)?;
        assert!(content.contains("# My shell config"));
        assert!(content.contains("export FOO=bar"));
        assert!(content.contains(MARKER_START));

        Ok(())
    }

    #[test]
    fn test_uninstall_removes_section() -> io::Result<()> {
        let temp = TempDir::new()?;
        let rc_file = temp.path().join(".zshrc");
        let script_path = PathBuf::from("/path/to/agr.sh");

        // Create RC file with existing content
        fs::write(&rc_file, "# My shell config\nexport FOO=bar\n")?;

        // Install then uninstall
        install(&rc_file, &script_path)?;
        let removed = uninstall(&rc_file)?;

        // Verify section removed
        assert!(removed);
        let content = fs::read_to_string(&rc_file)?;
        assert!(!content.contains(MARKER_START));
        assert!(!content.contains(MARKER_END));
        assert!(content.contains("export FOO=bar"));

        Ok(())
    }

    #[test]
    fn test_uninstall_returns_false_when_not_installed() -> io::Result<()> {
        let temp = TempDir::new()?;
        let rc_file = temp.path().join(".zshrc");

        fs::write(&rc_file, "# Just a normal config\n")?;

        let removed = uninstall(&rc_file)?;
        assert!(!removed);

        Ok(())
    }

    #[test]
    fn test_is_installed_in_detects_markers() -> io::Result<()> {
        let temp = TempDir::new()?;
        let rc_file = temp.path().join(".zshrc");

        // Not installed
        fs::write(&rc_file, "# Normal config\n")?;
        assert!(!is_installed_in(&rc_file)?);

        // With markers
        let script_path = PathBuf::from("/path/to/agr.sh");
        install(&rc_file, &script_path)?;
        assert!(is_installed_in(&rc_file)?);

        Ok(())
    }

    #[test]
    fn test_extract_script_path() -> io::Result<()> {
        let temp = TempDir::new()?;
        let rc_file = temp.path().join(".zshrc");
        let script_path = PathBuf::from("/custom/path/to/agr.sh");

        install(&rc_file, &script_path)?;

        let extracted = extract_script_path(&rc_file)?;
        assert_eq!(extracted, Some(script_path));

        Ok(())
    }

    #[test]
    fn test_install_replaces_existing_section() -> io::Result<()> {
        let temp = TempDir::new()?;
        let rc_file = temp.path().join(".zshrc");

        // Install with old path
        let old_path = PathBuf::from("/old/path/agr.sh");
        install(&rc_file, &old_path)?;

        // Install with new path
        let new_path = PathBuf::from("/new/path/agr.sh");
        install(&rc_file, &new_path)?;

        // Verify only new path present
        let content = fs::read_to_string(&rc_file)?;
        assert!(!content.contains("/old/path"));
        assert!(content.contains("/new/path"));

        // Verify only one set of markers
        assert_eq!(content.matches(MARKER_START).count(), 1);
        assert_eq!(content.matches(MARKER_END).count(), 1);

        Ok(())
    }

    #[test]
    fn test_status_summary() {
        let status = ShellStatus {
            rc_file: Some(PathBuf::from("/home/user/.zshrc")),
            script_path: Some(PathBuf::from("/home/user/.config/asr/agr.sh")),
            auto_wrap_enabled: true,
            is_active: false,
        };

        let summary = status.summary();
        assert!(summary.contains(".zshrc"));
        assert!(summary.contains("enabled"));
        assert!(summary.contains("installed"));
    }

    #[test]
    fn test_shell_script_is_embedded() {
        assert!(!SHELL_SCRIPT.is_empty());
        assert!(SHELL_SCRIPT.contains("_agr_record_session"));
    }

    #[test]
    fn test_install_script_writes_file() -> io::Result<()> {
        let temp = TempDir::new()?;
        let script_path = temp.path().join("agr.sh");

        install_script(&script_path)?;

        assert!(script_path.exists());
        let content = fs::read_to_string(&script_path)?;
        assert_eq!(content, SHELL_SCRIPT);

        Ok(())
    }
}
