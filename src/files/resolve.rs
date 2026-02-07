//! File path resolution for cast files.
//!
//! Supports multiple path formats:
//! 1. Absolute paths: /path/to/file.cast
//! 2. Short format: agent/file.cast
//! 3. Filename only: file.cast (fuzzy matches across all agents)

use std::path::PathBuf;

use anyhow::Result;

use crate::config::Config;
use crate::storage::StorageManager;

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

        create_test_session(temp.path(), "claude", "session.cast");
        let abs_path = temp.path().join("claude").join("session.cast");

        let result = resolve_file_path(&abs_path.to_string_lossy(), &config).unwrap();
        assert_eq!(result, abs_path);
    }

    #[test]
    fn resolve_short_format_agent_slash_file() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "test-session.cast");

        let result = resolve_file_path("claude/test-session.cast", &config).unwrap();
        let expected = temp.path().join("claude").join("test-session.cast");
        assert_eq!(result, expected);
    }

    #[test]
    fn resolve_filename_only_fuzzy_match() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "codex", "unique-session.cast");

        let result = resolve_file_path("unique-session.cast", &config).unwrap();
        let expected = temp.path().join("codex").join("unique-session.cast");
        assert_eq!(result, expected);
    }

    #[test]
    fn resolve_returns_original_when_not_found() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        let result = resolve_file_path("nonexistent.cast", &config).unwrap();
        assert_eq!(result, PathBuf::from("nonexistent.cast"));
    }

    #[test]
    fn resolve_short_format_not_found_returns_original() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        fs::create_dir_all(temp.path().join("claude")).unwrap();

        let result = resolve_file_path("claude/missing.cast", &config).unwrap();
        assert_eq!(result, PathBuf::from("claude/missing.cast"));
    }

    #[test]
    fn resolve_with_slash_does_not_fuzzy_match() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "codex", "session.cast");

        let result = resolve_file_path("claude/session.cast", &config).unwrap();
        assert_eq!(result, PathBuf::from("claude/session.cast"));
    }
}
