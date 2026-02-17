//! End-to-end integration tests for cast file import functionality

use agr::storage::StorageManager;
use agr::tui::import::parse_paste_paths;
use agr::Config;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a valid v3 cast file for testing
fn create_valid_cast(path: &Path) {
    let mut file = fs::File::create(path).unwrap();
    writeln!(file, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();
    writeln!(file, r#"[0.1,"o","test output"]"#).unwrap();
    writeln!(file, r#"[1.5,"o","more output"]"#).unwrap();
}

/// Helper to create test config with temp directory
fn create_test_config(temp_dir: &TempDir) -> Config {
    let mut config = Config::default();
    config.storage.directory = temp_dir.path().to_string_lossy().to_string();
    config
}

// ========================================================================
// End-to-End Import Tests
// ========================================================================

#[test]
fn end_to_end_import_single_file() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a cast file to import
    let source = temp.path().join("external.cast");
    create_valid_cast(&source);

    // Import into managed storage
    let result = manager.import_cast_file(&source, "test-agent");
    assert!(result.is_ok(), "Import should succeed");

    let imported_path = result.unwrap();

    // Verify file exists in correct location
    assert!(imported_path.exists(), "Imported file should exist");
    assert!(
        imported_path
            .to_string_lossy()
            .contains("test-agent/external.cast"),
        "File should be in test-agent directory with original name"
    );

    // Verify content was copied correctly
    let imported_content = fs::read_to_string(&imported_path).unwrap();
    let source_content = fs::read_to_string(&source).unwrap();
    assert_eq!(
        imported_content, source_content,
        "Imported file content should match source"
    );

    // Verify file is discoverable via list_sessions
    let sessions = manager.list_sessions(Some("test-agent")).unwrap();
    assert_eq!(sessions.len(), 1, "Should find one session");
    assert_eq!(sessions[0].filename, "external.cast");
}

#[test]
fn end_to_end_import_multiple_files() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create multiple cast files
    let files = vec!["session1.cast", "session2.cast", "session3.cast"];
    let mut sources = Vec::new();

    for filename in &files {
        let source = temp.path().join(filename);
        create_valid_cast(&source);
        sources.push(source);
    }

    // Import all files into same agent
    let mut results = Vec::new();
    for source in &sources {
        let result = manager.import_cast_file(source, "multi-import");
        results.push(result);
    }

    // Verify all imports succeeded
    assert_eq!(results.len(), 3);
    for result in &results {
        assert!(result.is_ok(), "All imports should succeed");
    }

    // Verify all files are discoverable
    let sessions = manager.list_sessions(Some("multi-import")).unwrap();
    assert_eq!(sessions.len(), 3, "Should find three sessions");

    let filenames: Vec<_> = sessions.iter().map(|s| s.filename.as_str()).collect();
    assert!(filenames.contains(&"session1.cast"));
    assert!(filenames.contains(&"session2.cast"));
    assert!(filenames.contains(&"session3.cast"));
}

#[test]
fn end_to_end_import_conflict_resolution() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a cast file
    let source = temp.path().join("duplicate.cast");
    create_valid_cast(&source);

    // Import same file twice
    let first = manager.import_cast_file(&source, "agent").unwrap();
    let second = manager.import_cast_file(&source, "agent").unwrap();

    // Both should exist
    assert!(first.exists(), "First import should exist");
    assert!(second.exists(), "Second import should exist");

    // Second should have -1 suffix
    assert!(first.ends_with("duplicate.cast"));
    assert!(
        second.to_string_lossy().contains("duplicate-1.cast"),
        "Second import should have -1 suffix"
    );

    // Verify both are discoverable
    let sessions = manager.list_sessions(Some("agent")).unwrap();
    assert_eq!(sessions.len(), 2, "Should find both sessions");
}

#[test]
fn end_to_end_import_invalid_file() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a file that's not a valid cast file
    let invalid = temp.path().join("invalid.cast");
    fs::write(&invalid, "not a valid cast file").unwrap();

    // Import should fail
    let result = manager.import_cast_file(&invalid, "agent");
    assert!(result.is_err(), "Import of invalid file should fail");

    // Error should be about invalid format
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("Invalid format") || error_msg.contains("invalid"),
        "Error should mention invalid format: {}",
        error_msg
    );
}

#[test]
fn end_to_end_import_wrong_extension() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Create a valid cast file but with wrong extension
    let wrong_ext = temp.path().join("session.txt");
    let mut file = fs::File::create(&wrong_ext).unwrap();
    writeln!(file, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();

    // Import should fail
    let result = manager.import_cast_file(&wrong_ext, "agent");
    assert!(result.is_err(), "Import of wrong extension should fail");

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("Wrong extension") || error_msg.contains("extension"),
        "Error should mention extension: {}",
        error_msg
    );
}

#[test]
fn end_to_end_import_nonexistent_file() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Try to import a file that doesn't exist
    let missing = temp.path().join("nonexistent.cast");

    let result = manager.import_cast_file(&missing, "agent");
    assert!(result.is_err(), "Import of missing file should fail");

    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("not found") || error_msg.contains("Not found"),
        "Error should mention file not found: {}",
        error_msg
    );
}

#[test]
fn end_to_end_import_creates_agent_directory() {
    let temp = TempDir::new().unwrap();
    let config = create_test_config(&temp);
    let manager = StorageManager::new(config);

    // Agent directory doesn't exist yet
    let new_agent_dir = temp.path().join("brand-new-agent");
    assert!(!new_agent_dir.exists(), "Agent dir should not exist yet");

    // Create and import a cast file
    let source = temp.path().join("test.cast");
    create_valid_cast(&source);

    let result = manager.import_cast_file(&source, "brand-new-agent");
    assert!(result.is_ok(), "Import should succeed");

    // Agent directory should now exist
    assert!(
        new_agent_dir.exists(),
        "Import should create agent directory"
    );

    // File should be in that directory
    let imported = result.unwrap();
    assert!(
        imported.starts_with(&new_agent_dir),
        "Imported file should be in new agent directory"
    );
}

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
