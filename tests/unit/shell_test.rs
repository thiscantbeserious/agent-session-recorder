//! Unit tests for shell module

use agr::shell::{
    extract_script_path, generate_section, install, install_script, is_installed_in, uninstall,
    SHELL_SCRIPT,
};
use agr::ShellStatus;
use std::fs;
use std::io;
use std::path::PathBuf;
use tempfile::TempDir;

const MARKER_START: &str = "# >>> AGR (Agent Session Recorder) >>>";
const MARKER_END: &str = "# <<< AGR (Agent Session Recorder) <<<";
const MARKER_WARNING: &str = "# DO NOT EDIT - managed by 'agr shell install/uninstall'";

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
