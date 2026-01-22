//! Unit tests for shell module

use agr::shell::{
    extract_script_path, generate_section, install, is_installed_in, uninstall, SHELL_SCRIPT,
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
    let section = generate_section();

    assert!(section.contains(MARKER_START));
    assert!(section.contains(MARKER_END));
    assert!(section.contains(MARKER_WARNING));
}

#[test]
fn test_generate_section_embeds_full_script() {
    let section = generate_section();

    // Should contain the full embedded script content, not a source line
    assert!(section.contains("_agr_record_session"));
    assert!(section.contains("_agr_setup_wrappers"));
    assert!(section.contains("_AGR_LOADED=1"));
    // Should NOT contain the old-style external source line pattern
    // (but may contain source lines within the embedded script for completions)
    assert!(!section.contains("[ -f \"") || section.contains("_agr_setup_completions"));
}

#[test]
fn test_install_creates_section_in_rc() -> io::Result<()> {
    let temp = TempDir::new()?;
    let rc_file = temp.path().join(".zshrc");

    // Create empty RC file
    fs::write(&rc_file, "")?;

    // Install
    install(&rc_file)?;

    // Verify
    let content = fs::read_to_string(&rc_file)?;
    assert!(content.contains(MARKER_START));
    assert!(content.contains(MARKER_END));
    // Should contain embedded script content, not source line
    assert!(content.contains("_agr_record_session"));
    assert!(content.contains("_AGR_LOADED=1"));

    Ok(())
}

#[test]
fn test_install_appends_to_existing_content() -> io::Result<()> {
    let temp = TempDir::new()?;
    let rc_file = temp.path().join(".zshrc");

    // Create RC file with existing content
    fs::write(&rc_file, "# My shell config\nexport FOO=bar\n")?;

    // Install
    install(&rc_file)?;

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

    // Create RC file with existing content
    fs::write(&rc_file, "# My shell config\nexport FOO=bar\n")?;

    // Install then uninstall
    install(&rc_file)?;
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
    install(&rc_file)?;
    assert!(is_installed_in(&rc_file)?);

    Ok(())
}

#[test]
fn test_extract_script_path_returns_none_for_embedded() -> io::Result<()> {
    let temp = TempDir::new()?;
    let rc_file = temp.path().join(".zshrc");

    // New embedded installation should not have a script path
    install(&rc_file)?;

    let extracted = extract_script_path(&rc_file)?;
    assert_eq!(extracted, None);

    Ok(())
}

#[test]
fn test_extract_script_path_handles_old_style_installation() -> io::Result<()> {
    let temp = TempDir::new()?;
    let rc_file = temp.path().join(".zshrc");

    // Simulate old-style installation with source line
    let old_style_content = r#"# >>> AGR (Agent Session Recorder) >>>
# DO NOT EDIT - managed by 'agr shell install/uninstall'
[ -f "/home/user/.config/agr/agr.sh" ] && source "/home/user/.config/agr/agr.sh"
# <<< AGR (Agent Session Recorder) <<<"#;
    fs::write(&rc_file, old_style_content)?;

    let extracted = extract_script_path(&rc_file)?;
    assert_eq!(
        extracted,
        Some(PathBuf::from("/home/user/.config/agr/agr.sh"))
    );

    Ok(())
}

#[test]
fn test_install_replaces_existing_section() -> io::Result<()> {
    let temp = TempDir::new()?;
    let rc_file = temp.path().join(".zshrc");

    // First install
    install(&rc_file)?;

    // Add some custom content after
    let content = fs::read_to_string(&rc_file)?;
    fs::write(&rc_file, format!("{content}\n# Custom content after AGR\n"))?;

    // Install again (should replace existing section)
    install(&rc_file)?;

    // Verify only one set of markers
    let content = fs::read_to_string(&rc_file)?;
    assert_eq!(content.matches(MARKER_START).count(), 1);
    assert_eq!(content.matches(MARKER_END).count(), 1);

    // Custom content should still be there (it was after the section)
    // Note: the uninstall removes content between markers, so custom content after is preserved
    assert!(content.contains("# Custom content after AGR"));

    Ok(())
}

#[test]
fn test_install_upgrades_old_style_installation() -> io::Result<()> {
    let temp = TempDir::new()?;
    let rc_file = temp.path().join(".zshrc");

    // Simulate old-style installation with source line
    let old_style_content = r#"# My shell config
export FOO=bar

# >>> AGR (Agent Session Recorder) >>>
# DO NOT EDIT - managed by 'agr shell install/uninstall'
[ -f "/home/user/.config/agr/agr.sh" ] && source "/home/user/.config/agr/agr.sh"
# <<< AGR (Agent Session Recorder) <<<
"#;
    fs::write(&rc_file, old_style_content)?;

    // Install should upgrade to embedded style
    install(&rc_file)?;

    // Verify new embedded style
    let content = fs::read_to_string(&rc_file)?;
    assert!(content.contains("export FOO=bar")); // Preserve existing content
    assert!(content.contains("_agr_record_session")); // Embedded script
    assert!(!content.contains("[ -f \"/home/user")); // No old source line
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
