//! Completions command handler

use anyhow::{anyhow, Result};
use clap::CommandFactory;
use clap_complete::{generate, Shell as CompletionShell};
use std::io;

use agr::{shell, Config, StorageManager};

/// Handle completions command.
///
/// Generates shell completion scripts or lists cast files for dynamic completion.
#[cfg(not(tarpaulin_include))]
pub fn handle<C: CommandFactory>(
    shell_arg: Option<CompletionShell>,
    shell_init: Option<CompletionShell>,
    debug: bool,
    files: bool,
    limit: usize,
    prefix: &str,
) -> Result<()> {
    // Handle --shell-init first (new dynamic generation)
    if let Some(shell) = shell_init {
        let output = match shell {
            CompletionShell::Zsh => shell::generate_zsh_init(debug),
            CompletionShell::Bash => shell::generate_bash_init(debug),
            _ => return Err(anyhow!("Only zsh and bash are supported for --shell-init")),
        };
        println!("{}", output);
        return Ok(());
    }

    // Handle --files (dynamic file listing)
    if files {
        return list_cast_files(prefix, limit);
    }

    // Handle --shell (clap native completions)
    if let Some(shell) = shell_arg {
        return generate_completions::<C>(shell);
    }

    // No arguments - show usage
    eprintln!("Usage: agr completions --shell <bash|zsh|fish|powershell>");
    eprintln!("       agr completions --shell-init <bash|zsh>");
    eprintln!("       agr completions --files [prefix]");
    std::process::exit(1);
}

/// List cast files for dynamic completion.
fn list_cast_files(prefix: &str, limit: usize) -> Result<()> {
    let config = Config::load()?;
    list_cast_files_with_config(prefix, limit, &config)
}

/// List cast files using the provided config (for testing).
pub(crate) fn list_cast_files_with_config(
    prefix: &str,
    limit: usize,
    config: &Config,
) -> Result<()> {
    let storage = StorageManager::new(config.clone());

    let prefix_filter = if prefix.is_empty() {
        None
    } else {
        Some(prefix)
    };

    let files = storage.list_cast_files_short(prefix_filter)?;
    for file in files.into_iter().take(limit) {
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

        let result = list_cast_files_with_config("", 10, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn list_cast_files_with_config_with_sessions_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "session1.cast");
        create_test_session(temp.path(), "codex", "session2.cast");

        let result = list_cast_files_with_config("", 10, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn list_cast_files_with_config_with_prefix_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "session1.cast");
        create_test_session(temp.path(), "codex", "session2.cast");

        let result = list_cast_files_with_config("claude/", 10, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn list_cast_files_with_config_nonexistent_prefix_does_not_panic() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        create_test_session(temp.path(), "claude", "session1.cast");

        let result = list_cast_files_with_config("nonexistent/", 10, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn list_cast_files_with_config_respects_limit() {
        let temp = TempDir::new().unwrap();
        let config = create_test_config(&temp);

        // Create more files than the limit
        for i in 0..5 {
            create_test_session(temp.path(), "claude", &format!("session{}.cast", i));
        }

        // We can't easily capture stdout in tests, but we can at least verify it runs
        let result = list_cast_files_with_config("", 3, &config);
        assert!(result.is_ok());
    }
}
