//! End-to-end integration tests for cast file import functionality

use agr::storage::StorageManager;
use agr::Config;
use std::fs;
use std::io::Write;
use std::path::Path;
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
    config.storage.compression = agr::config::Compression::None;
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
