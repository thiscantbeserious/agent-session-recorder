//! Path traversal and agent name validation tests for `import_cast_file`.

use super::helpers::storage::create_dir_config;
use agr::storage::ImportError;
use agr::StorageManager;
use std::fs;
use std::io::Write;
use tempfile::TempDir;

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
