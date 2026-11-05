//! Tests for `parse_paste_paths` with realistic paste scenarios.

use agr::tui::import::parse_paste_paths;
use std::path::PathBuf;

// ========================================================================
// Parse Paste Paths - Realistic Scenarios
// ========================================================================

#[test]
fn parse_paste_single_absolute_path() {
    let paste = "/Users/test/recordings/session.cast";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 1);
    assert_eq!(
        paths[0],
        PathBuf::from("/Users/test/recordings/session.cast")
    );
}

#[test]
fn parse_paste_multiple_paths_newline_separated() {
    let paste = "/Users/test/session1.cast\n/Users/test/session2.cast\n/Users/test/session3.cast";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0], PathBuf::from("/Users/test/session1.cast"));
    assert_eq!(paths[1], PathBuf::from("/Users/test/session2.cast"));
    assert_eq!(paths[2], PathBuf::from("/Users/test/session3.cast"));
}

#[test]
fn parse_paste_macos_finder_format() {
    // macOS Finder copies paths with single quotes and spaces
    let paste = "'/Users/simon/Downloads/my session.cast'";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 1);
    assert_eq!(
        paths[0],
        PathBuf::from("/Users/simon/Downloads/my session.cast")
    );
}

#[test]
fn parse_paste_quoted_paths_with_spaces() {
    let paste = r#""/home/user/My Documents/recording.cast"
"/home/user/Projects/test session.cast""#;
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 2);
    assert_eq!(
        paths[0],
        PathBuf::from("/home/user/My Documents/recording.cast")
    );
    assert_eq!(
        paths[1],
        PathBuf::from("/home/user/Projects/test session.cast")
    );
}

#[test]
fn parse_paste_mixed_quoted_and_unquoted() {
    let paste = "/Users/test/simple.cast\n'/Users/test/with spaces.cast'\n/Users/test/another.cast";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0], PathBuf::from("/Users/test/simple.cast"));
    assert_eq!(paths[1], PathBuf::from("/Users/test/with spaces.cast"));
    assert_eq!(paths[2], PathBuf::from("/Users/test/another.cast"));
}

#[test]
fn parse_paste_tilde_expansion() {
    let paste = "~/recordings/session.cast";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 1);

    // Should expand to home directory
    let home = dirs::home_dir().expect("HOME must be set for this test");
    let expected = home.join("recordings/session.cast");
    assert_eq!(paths[0], expected);
}

#[test]
fn parse_paste_empty_lines_ignored() {
    let paste = "/Users/test/first.cast\n\n\n/Users/test/second.cast\n\n";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], PathBuf::from("/Users/test/first.cast"));
    assert_eq!(paths[1], PathBuf::from("/Users/test/second.cast"));
}

#[test]
fn parse_paste_whitespace_trimmed() {
    let paste = "  /Users/test/session.cast  \n  /Users/test/another.cast  ";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0], PathBuf::from("/Users/test/session.cast"));
    assert_eq!(paths[1], PathBuf::from("/Users/test/another.cast"));
}

#[test]
fn parse_paste_terminal_ls_output() {
    // Simulates pasting output from: ls ~/recordings/*.cast
    let paste = "/Users/test/recordings/session1.cast\n/Users/test/recordings/session2.cast";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 2);
    assert!(paths[0].ends_with("session1.cast"));
    assert!(paths[1].ends_with("session2.cast"));
}

#[test]
fn parse_paste_empty_string() {
    let paste = "";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 0);
}

#[test]
fn parse_paste_only_whitespace() {
    let paste = "   \n\n   \n  ";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 0);
}

#[test]
fn parse_paste_windows_style_paths() {
    let paste = r"C:\Users\test\session.cast";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 1);
    // On Windows this would be a valid absolute path
    // On Unix it's relative, but we still parse it
    assert!(paths[0].to_string_lossy().contains("session.cast"));
}

#[test]
fn parse_paste_relative_paths_resolved() {
    // Relative paths should be resolved against current working directory
    let paste = "recordings/session.cast";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 1);

    // Should be absolute path now
    assert!(
        paths[0].is_absolute(),
        "Relative path should be resolved to absolute"
    );
    assert!(paths[0].ends_with("recordings/session.cast"));
}

#[test]
fn parse_paste_drag_drop_simulator() {
    // Simulates what a terminal emulator might paste when files are dragged in
    // Different terminals escape differently, but most quote paths with spaces
    // Single-line space-separated won't work since we split on newlines
    // Test the realistic newline-separated version:
    let paste = "'/tmp/my recording.cast'\n/tmp/simple.cast\n'/tmp/another file.cast'";
    let paths = parse_paste_paths(paste);

    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0], PathBuf::from("/tmp/my recording.cast"));
    assert_eq!(paths[1], PathBuf::from("/tmp/simple.cast"));
    assert_eq!(paths[2], PathBuf::from("/tmp/another file.cast"));
}
