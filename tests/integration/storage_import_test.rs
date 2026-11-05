//! Storage tests: cast header validation and import

use super::helpers::storage::{compression_config, create_dir_config};
use agr::config::Compression;
use agr::storage::{validate_cast_header, ImportError};
use agr::StorageManager;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::TempDir;

// ========================================================================
// Import functionality tests (Stage 2)
// ========================================================================

/// Helper to create a valid v3 cast file for testing
fn create_valid_v3_cast(path: &Path) {
    let mut file = fs::File::create(path).unwrap();
    writeln!(file, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();
    writeln!(file, r#"[0.1,"o","test output"]"#).unwrap();
}

/// Helper to create a valid v2 cast file for testing
fn create_valid_v2_cast(path: &Path) {
    let mut file = fs::File::create(path).unwrap();
    writeln!(file, r#"{{"version":2,"width":80,"height":24}}"#).unwrap();
    writeln!(file, r#"[0.1,"o","test output"]"#).unwrap();
}

#[test]
fn validate_cast_header_valid_v3() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("test.cast");
    create_valid_v3_cast(&cast_file);

    let result = validate_cast_header(&cast_file);
    assert!(result.is_ok(), "Valid v3 file should pass validation");
}

#[test]
fn validate_cast_header_valid_v2() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("test.cast");
    create_valid_v2_cast(&cast_file);

    let result = validate_cast_header(&cast_file);
    assert!(result.is_ok(), "Valid v2 file should pass validation");
}

#[test]
fn validate_cast_header_missing_file() {
    let temp = TempDir::new().unwrap();
    let missing_file = temp.path().join("nonexistent.cast");

    let result = validate_cast_header(&missing_file);
    assert!(result.is_err(), "Missing file should fail validation");
    assert!(
        matches!(result.unwrap_err(), ImportError::NotFound(_)),
        "Should return NotFound error"
    );
}

#[test]
fn validate_cast_header_wrong_extension() {
    let temp = TempDir::new().unwrap();
    let txt_file = temp.path().join("test.txt");
    let mut file = fs::File::create(&txt_file).unwrap();
    writeln!(file, r#"{{"version":3}}"#).unwrap();

    let result = validate_cast_header(&txt_file);
    assert!(result.is_err(), "Wrong extension should fail validation");
    assert!(
        matches!(result.unwrap_err(), ImportError::WrongExtension(_)),
        "Should return WrongExtension error"
    );
}

#[test]
fn validate_cast_header_invalid_json() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("invalid.cast");
    let mut file = fs::File::create(&cast_file).unwrap();
    writeln!(file, "not valid json").unwrap();

    let result = validate_cast_header(&cast_file);
    assert!(result.is_err(), "Invalid JSON should fail validation");
    assert!(
        matches!(result.unwrap_err(), ImportError::InvalidFormat(_)),
        "Should return InvalidFormat error"
    );
}

#[test]
fn validate_cast_header_no_version() {
    let temp = TempDir::new().unwrap();
    let cast_file = temp.path().join("no_version.cast");
    let mut file = fs::File::create(&cast_file).unwrap();
    writeln!(file, r#"{{"width":80,"height":24}}"#).unwrap();

    let result = validate_cast_header(&cast_file);
    assert!(
        result.is_err(),
        "JSON without version should fail validation"
    );
    assert!(
        matches!(result.unwrap_err(), ImportError::InvalidFormat(_)),
        "Should return InvalidFormat error"
    );
}

#[test]
fn import_cast_file_success() {
    let temp = TempDir::new().unwrap();
    let config = compression_config(&temp, Compression::None);
    let manager = StorageManager::new(config);

    // Create a valid cast file to import
    let source = temp.path().join("source.cast");
    create_valid_v3_cast(&source);

    // Import it
    let result = manager.import_cast_file(&source, "claude");
    assert!(result.is_ok(), "Import should succeed");

    let imported_path = result.unwrap();
    assert!(imported_path.exists(), "Imported file should exist");
    assert!(
        imported_path.to_string_lossy().contains("claude"),
        "Should be in claude directory"
    );
    assert!(
        imported_path.ends_with("source.cast"),
        "Should preserve filename"
    );
}

#[test]
fn import_cast_file_creates_agent_dir() {
    let temp = TempDir::new().unwrap();
    let config = compression_config(&temp, Compression::None);
    let manager = StorageManager::new(config);

    // Create a valid cast file
    let source = temp.path().join("test.cast");
    create_valid_v3_cast(&source);

    // Import into non-existing agent directory
    let result = manager.import_cast_file(&source, "new-agent");
    assert!(result.is_ok(), "Import should create agent directory");

    let imported_path = result.unwrap();
    assert!(imported_path.exists(), "Imported file should exist");

    let agent_dir = temp.path().join("new-agent");
    assert!(agent_dir.exists(), "Agent directory should be created");
}

#[test]
fn import_cast_file_conflict_resolution() {
    let temp = TempDir::new().unwrap();
    let config = compression_config(&temp, Compression::None);
    let manager = StorageManager::new(config);

    // Create source file
    let source = temp.path().join("session.cast");
    create_valid_v3_cast(&source);

    // First import
    let first_import = manager.import_cast_file(&source, "claude").unwrap();
    assert!(first_import.ends_with("session.cast"));

    // Second import of same file - should get -1 suffix
    let second_import = manager.import_cast_file(&source, "claude").unwrap();
    assert!(
        second_import.to_string_lossy().contains("session-1.cast"),
        "Second import should have -1 suffix, got: {}",
        second_import.display()
    );

    // Both files should exist
    assert!(first_import.exists());
    assert!(second_import.exists());
}

#[test]
fn import_cast_file_preserves_filename() {
    let temp = TempDir::new().unwrap();
    let config = compression_config(&temp, Compression::None);
    let manager = StorageManager::new(config);

    // Create source with specific filename
    let source = temp.path().join("my-special-session.cast");
    create_valid_v3_cast(&source);

    // Import it
    let imported = manager.import_cast_file(&source, "claude").unwrap();

    // Should preserve the original filename
    assert!(
        imported.ends_with("my-special-session.cast"),
        "Should preserve original filename"
    );
}

#[test]
fn resolve_filename_conflict_no_conflict() {
    let temp = TempDir::new().unwrap();

    // Create test using the internal helper via import
    let config = compression_config(&temp, Compression::None);
    let manager = StorageManager::new(config);

    let source = temp.path().join("unique.cast");
    create_valid_v3_cast(&source);

    let imported = manager.import_cast_file(&source, "claude").unwrap();
    assert!(
        imported.ends_with("unique.cast"),
        "Should use original filename when no conflict"
    );
}

#[test]
fn resolve_filename_conflict_with_existing() {
    let temp = TempDir::new().unwrap();
    let config = compression_config(&temp, Compression::None);
    let manager = StorageManager::new(config);

    let source = temp.path().join("conflict.cast");
    create_valid_v3_cast(&source);

    // First import
    let first = manager.import_cast_file(&source, "claude").unwrap();
    assert!(first.ends_with("conflict.cast"));

    // Second import - should get -1 suffix
    let second = manager.import_cast_file(&source, "claude").unwrap();
    assert!(second.to_string_lossy().contains("conflict-1.cast"));

    // Third import - should get -2 suffix
    let third = manager.import_cast_file(&source, "claude").unwrap();
    assert!(third.to_string_lossy().contains("conflict-2.cast"));
}
// ========================================================================
// Path Traversal Security Tests
// ========================================================================

/// Test that import_cast_file rejects agent names with path traversal attempts.
#[test]
fn import_cast_file_rejects_path_traversal() {
    let temp = TempDir::new().unwrap();
    let storage_dir = temp.path();

    // Create a valid cast file to import
    let source = temp.path().join("test.cast");
    let mut f = fs::File::create(&source).unwrap();
    writeln!(f, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();
    writeln!(f, r#"[0.1,"o","test"]"#).unwrap();

    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);

    // Test various path traversal attempts
    let malicious_names = vec![
        "../../etc",
        "../../../root",
        "..\\..\\windows",
        "agent/../etc",
        "./../../tmp",
    ];

    for agent_name in malicious_names {
        let result = manager.import_cast_file(&source, agent_name);
        assert!(
            result.is_err(),
            "Should reject path traversal attempt: {}",
            agent_name
        );

        // Verify error is InvalidAgentName
        match result {
            Err(ImportError::InvalidAgentName(_)) => {
                // Expected error type
            }
            other => panic!(
                "Expected InvalidAgentName error for '{}', got: {:?}",
                agent_name, other
            ),
        }
    }
}

/// Test that import_cast_file rejects empty or whitespace-only agent names.
#[test]
fn import_cast_file_rejects_empty_agent() {
    let temp = TempDir::new().unwrap();
    let storage_dir = temp.path();

    // Create a valid cast file
    let source = temp.path().join("test.cast");
    let mut f = fs::File::create(&source).unwrap();
    writeln!(f, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();
    writeln!(f, r#"[0.1,"o","test"]"#).unwrap();

    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);

    // Test empty and whitespace-only names
    let invalid_names = vec!["", "   ", "\t", "\n", "  \n\t  "];

    for agent_name in invalid_names {
        let result = manager.import_cast_file(&source, agent_name);
        assert!(
            result.is_err(),
            "Should reject empty/whitespace agent name: {:?}",
            agent_name
        );

        // Verify error is InvalidAgentName
        match result {
            Err(ImportError::InvalidAgentName(_)) => {
                // Expected error type
            }
            other => panic!(
                "Expected InvalidAgentName error for '{:?}', got: {:?}",
                agent_name, other
            ),
        }
    }
}

/// Test that import_cast_file rejects agent names with slashes.
#[test]
fn import_cast_file_rejects_slashes() {
    let temp = TempDir::new().unwrap();
    let storage_dir = temp.path();

    // Create a valid cast file
    let source = temp.path().join("test.cast");
    let mut f = fs::File::create(&source).unwrap();
    writeln!(f, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();
    writeln!(f, r#"[0.1,"o","test"]"#).unwrap();

    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);

    // Test names with slashes
    let invalid_names = vec!["agent/subdir", "path\\to\\agent", "/absolute/path"];

    for agent_name in invalid_names {
        let result = manager.import_cast_file(&source, agent_name);
        assert!(
            result.is_err(),
            "Should reject agent name with slashes: {}",
            agent_name
        );

        match result {
            Err(ImportError::InvalidAgentName(_)) => {
                // Expected error type
            }
            other => panic!(
                "Expected InvalidAgentName error for '{}', got: {:?}",
                agent_name, other
            ),
        }
    }
}

/// Test that import_cast_file accepts valid agent names.
#[test]
fn import_cast_file_accepts_valid_agent_names() {
    let temp = TempDir::new().unwrap();
    let storage_dir = temp.path();

    // Create a valid cast file
    let source = temp.path().join("test.cast");
    let mut f = fs::File::create(&source).unwrap();
    writeln!(f, r#"{{"version":3,"width":80,"height":24}}"#).unwrap();
    writeln!(f, r#"[0.1,"o","test"]"#).unwrap();

    let config = create_dir_config(storage_dir);
    let manager = StorageManager::new(config);

    // Test valid agent names
    let valid_names = vec![
        "claude",
        "agent-1",
        "my_agent",
        "AgentName123",
        "a",
        "test-agent_v2",
    ];

    for agent_name in valid_names {
        let result = manager.import_cast_file(&source, agent_name);
        assert!(
            result.is_ok(),
            "Should accept valid agent name '{}': {:?}",
            agent_name,
            result
        );
    }
}
