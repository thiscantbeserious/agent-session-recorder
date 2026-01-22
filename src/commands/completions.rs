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
    let storage = StorageManager::new(config);

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
fn generate_completions<C: CommandFactory>(shell: CompletionShell) -> Result<()> {
    let mut cmd = C::command();
    generate(shell, &mut cmd, "agr", &mut io::stdout());
    Ok(())
}
