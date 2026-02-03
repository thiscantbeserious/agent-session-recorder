//! Copy command handler

use anyhow::Result;

use agr::{clipboard::copy_file_to_clipboard, Config};

use super::resolve_file_path;

/// Copy a recording file to the system clipboard.
///
/// On macOS, copies as a file reference for paste-as-attachment.
/// On Linux, falls back to copying file content as text.
pub fn handle(file: &str) -> Result<()> {
    let config = Config::load()?;

    // Resolve file path (supports short format like "claude/session.cast")
    let filepath = resolve_file_path(file, &config)?;
    if !filepath.exists() {
        anyhow::bail!(
            "File not found: {}\nHint: Use format 'agent/file.cast'. Run 'agr list' to see available sessions.",
            file
        );
    }

    // Check file has .cast extension
    if filepath.extension().and_then(|e| e.to_str()) != Some("cast") {
        eprintln!("Warning: File does not have .cast extension");
    }

    // Copy to clipboard
    let result = copy_file_to_clipboard(&filepath)?;

    // Extract filename and strip .cast extension for the message
    let filename = filepath
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("recording");

    println!("{}", result.message(filename));
    Ok(())
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
        fs::write(&path, r#"{"version":3}"#).unwrap();
    }

    #[test]
    fn resolve_accepts_short_format() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "test.cast");

        let result = resolve_file_path("claude/test.cast", &config).unwrap();
        assert!(result.exists());
    }

    #[test]
    fn resolve_accepts_filename_only() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "codex", "unique-session.cast");

        let result = resolve_file_path("unique-session.cast", &config).unwrap();
        assert!(result.exists());
    }

    #[test]
    fn resolve_returns_path_for_nonexistent() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        // Returns the path even if it doesn't exist (error caught later)
        let result = resolve_file_path("nonexistent.cast", &config).unwrap();
        assert!(!result.exists());
    }
}
