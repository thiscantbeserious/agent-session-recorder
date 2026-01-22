//! Completions command handler

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::{generate, Shell as CompletionShell};
use std::io;

use agr::{Config, StorageManager};

/// Handle completions command.
///
/// Generates shell completion scripts or lists cast files for dynamic completion.
#[cfg(not(tarpaulin_include))]
pub fn handle<C: CommandFactory>(
    shell: Option<CompletionShell>,
    files: bool,
    prefix: &str,
) -> Result<()> {
    if files {
        return list_cast_files(prefix);
    }

    if let Some(shell) = shell {
        return generate_completions::<C>(shell);
    }

    // No arguments - show usage
    eprintln!("Usage: agr completions --shell <bash|zsh|fish|powershell>");
    eprintln!("       agr completions --files [prefix]");
    std::process::exit(1);
}

/// List cast files for dynamic completion.
fn list_cast_files(prefix: &str) -> Result<()> {
    let config = Config::load()?;
    list_cast_files_with_config(prefix, &config)
}

/// List cast files using the provided config (for testing).
pub(crate) fn list_cast_files_with_config(prefix: &str, config: &Config) -> Result<()> {
    let storage = StorageManager::new(config.clone());

    let prefix_filter = if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    };

    let files = storage.list_cast_files_short(prefix_filter)?;
    for file in files {
        println!("{}", file);
    }
    Ok(())
}

/// Generate shell completion script.
pub(crate) fn generate_completions<C: CommandFactory>(shell: CompletionShell) -> Result<()> {
    let mut cmd = C::command();
    generate(shell, &mut cmd, "agr", &mut io::stdout());
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
        fs::write(&path, "test content").unwrap();
    }

    #[test]
    fn list_cast_files_with_config_empty_storage_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        let result = list_cast_files_with_config("", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn list_cast_files_with_config_with_sessions_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "session1.cast");
        create_test_session(temp.path(), "codex", "session2.cast");

        let result = list_cast_files_with_config("", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn list_cast_files_with_config_with_prefix_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "session1.cast");
        create_test_session(temp.path(), "codex", "session2.cast");

        let result = list_cast_files_with_config("claude/", &config);
        assert!(result.is_ok());
    }

    #[test]
    fn list_cast_files_with_config_nonexistent_prefix_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "session1.cast");

        let result = list_cast_files_with_config("nonexistent/", &config);
        assert!(result.is_ok());
    }
}
