//! Command handlers for the AGR CLI.
//!
//! Each submodule handles a specific CLI command or command group.
//! The main dispatch logic remains in main.rs.

pub mod agents;
pub mod analyze;
pub mod cleanup;
pub mod completions;
pub mod config;
pub mod copy;
pub mod list;
pub mod marker;
pub mod play;
pub mod record;
pub mod shell;
pub mod status;
pub mod transform;

use anyhow::Result;
use std::path::PathBuf;

use agr::{Config, StorageManager};

/// Truncate a string to a maximum length, adding ellipsis if needed.
pub fn truncate_string(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len > 3 {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.chars().take(max_len).collect()
    }
}

/// Resolve a file path, trying short format (agent/file.cast) first.
///
/// Supports three formats:
/// 1. Absolute path: /path/to/file.cast
/// 2. Short format: agent/file.cast
/// 3. Filename only: file.cast (fuzzy matches across all agents)
pub fn resolve_file_path(file: &str, config: &Config) -> Result<PathBuf> {
    let path = PathBuf::from(file);

    // If it's already an absolute path or exists as-is, use it directly
    if path.is_absolute() || path.exists() {
        return Ok(path);
    }

    // Try to resolve as short format via StorageManager
    let storage = StorageManager::new(config.clone());

    if let Some(resolved) = storage.resolve_cast_path(file) {
        return Ok(resolved);
    }

    // If no "/" in path, try fuzzy matching across all agents
    if !file.contains('/') {
        if let Some(resolved) = storage.find_cast_file_by_name(file) {
            return Ok(resolved);
        }
    }

    // Return the original path (will fail later with appropriate error)
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_string_short_string_unchanged() {
        assert_eq!(truncate_string("hello", 10), "hello");
    }

    #[test]
    fn truncate_string_exact_length_unchanged() {
        assert_eq!(truncate_string("hello", 5), "hello");
    }

    #[test]
    fn truncate_string_long_string_with_ellipsis() {
        assert_eq!(truncate_string("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_string_very_short_max_len() {
        // When max_len <= 3, just truncate without ellipsis
        assert_eq!(truncate_string("hello", 3), "hel");
    }

    #[test]
    fn truncate_string_empty_string() {
        assert_eq!(truncate_string("", 10), "");
    }

    #[test]
    fn truncate_string_handles_multibyte_characters() {
        // Should not panic and should truncate by characters, not bytes
        assert_eq!(truncate_string("日本語テスト", 5), "日本...");
        assert_eq!(truncate_string("café", 10), "café");
        assert_eq!(truncate_string("emoji🎉test", 8), "emoji...");
    }

    // Tests for resolve_file_path function
    mod resolve_file_path_tests {
        use super::*;
        use std::fs;
        use tempfile::TempDir;

        fn create_test_config(temp_dir: &TempDir) -> Config {
            let mut config = Config::default();
            config.storage.directory = temp_dir.path().to_string_lossy().to_string();
            config
        }

        fn create_test_session(dir: &std::path::Path, agent: &str, filename: &str) {
            let agent_dir = dir.join(agent);
            fs::create_dir_all(&agent_dir).unwrap();
            let path = agent_dir.join(filename);
            fs::write(&path, "test content").unwrap();
        }

        #[test]
        fn resolve_absolute_path_that_exists() {
            let temp = TempDir::new().unwrap();
            let config = create_test_config(&temp);

            // Create a file
            create_test_session(temp.path(), "claude", "session.cast");
            let abs_path = temp.path().join("claude").join("session.cast");

            // Resolve should return the same path
            let result = resolve_file_path(&abs_path.to_string_lossy(), &config).unwrap();
            assert_eq!(result, abs_path);
        }

        #[test]
        fn resolve_short_format_agent_slash_file() {
            let temp = TempDir::new().unwrap();
            let config = create_test_config(&temp);

            // Create a file in the storage directory
            create_test_session(temp.path(), "claude", "test-session.cast");

            // Resolve using short format
            let result = resolve_file_path("claude/test-session.cast", &config).unwrap();
            let expected = temp.path().join("claude").join("test-session.cast");
            assert_eq!(result, expected);
        }

        #[test]
        fn resolve_filename_only_fuzzy_match() {
            let temp = TempDir::new().unwrap();
            let config = create_test_config(&temp);

            // Create a file
            create_test_session(temp.path(), "codex", "unique-session.cast");

            // Resolve using just the filename (no slash)
            let result = resolve_file_path("unique-session.cast", &config).unwrap();
            let expected = temp.path().join("codex").join("unique-session.cast");
            assert_eq!(result, expected);
        }

        #[test]
        fn resolve_returns_original_when_not_found() {
            let temp = TempDir::new().unwrap();
            let config = create_test_config(&temp);

            // Don't create any files
            // resolve_file_path should return the original path when nothing is found
            let result = resolve_file_path("nonexistent.cast", &config).unwrap();
            assert_eq!(result, PathBuf::from("nonexistent.cast"));
        }

        #[test]
        fn resolve_short_format_not_found_returns_original() {
            let temp = TempDir::new().unwrap();
            let config = create_test_config(&temp);

            // Create storage directory but not the file
            fs::create_dir_all(temp.path().join("claude")).unwrap();

            let result = resolve_file_path("claude/missing.cast", &config).unwrap();
            // Since it's not found, returns the original path
            assert_eq!(result, PathBuf::from("claude/missing.cast"));
        }

        #[test]
        fn resolve_with_slash_does_not_fuzzy_match() {
            let temp = TempDir::new().unwrap();
            let config = create_test_config(&temp);

            // Create a file in codex directory
            create_test_session(temp.path(), "codex", "session.cast");

            // Try to resolve with wrong agent path - should NOT find via fuzzy match
            // because the path contains a slash
            let result = resolve_file_path("claude/session.cast", &config).unwrap();
            // Since claude/session.cast doesn't exist, it returns original path
            assert_eq!(result, PathBuf::from("claude/session.cast"));
        }
    }
}
