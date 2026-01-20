//! Agent Session Recorder (ASR) - CLI entry point

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{self, BufRead, Write};

use asr::{Config, MarkerManager, Recorder, StorageManager};

#[derive(Parser)]
#[command(name = "asr")]
#[command(about = "Agent Session Recorder - Record AI agent terminal sessions")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start recording a session
    Record {
        /// Agent name (e.g., claude, codex, gemini-cli)
        agent: String,
        /// Arguments to pass to the agent
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Show storage statistics
    Status,

    /// Interactive cleanup of old sessions
    Cleanup {
        /// Filter by agent name
        #[arg(long)]
        agent: Option<String>,
        /// Only show sessions older than N days
        #[arg(long)]
        older_than: Option<u32>,
    },

    /// List recorded sessions
    List {
        /// Filter by agent name
        agent: Option<String>,
    },

    /// Manage markers in cast files
    #[command(subcommand)]
    Marker(MarkerCommands),

    /// Manage configured agents
    #[command(subcommand)]
    Agents(AgentCommands),

    /// Configuration management
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Manage AI agent skills
    #[command(subcommand)]
    Skills(SkillsCommands),

    /// Manage shell integration
    #[command(subcommand)]
    Shell(ShellCommands),
}

#[derive(Subcommand)]
enum MarkerCommands {
    /// Add a marker to a cast file
    Add {
        /// Path to the .cast file
        file: String,
        /// Timestamp in seconds
        time: f64,
        /// Marker label
        label: String,
    },
    /// List markers in a cast file
    List {
        /// Path to the .cast file
        file: String,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List configured agents
    List,
    /// Add an agent to the configuration
    Add {
        /// Agent name
        name: String,
    },
    /// Remove an agent from the configuration
    Remove {
        /// Agent name
        name: String,
    },
    /// Check if an agent should be wrapped (for shell integration)
    #[command(name = "is-wrapped")]
    IsWrapped {
        /// Agent name
        name: String,
    },
    /// Manage agents that should not be auto-wrapped
    #[command(subcommand)]
    NoWrap(NoWrapCommands),
}

#[derive(Subcommand)]
enum NoWrapCommands {
    /// List agents that are not auto-wrapped
    List,
    /// Add an agent to the no-wrap list
    Add {
        /// Agent name
        name: String,
    },
    /// Remove an agent from the no-wrap list
    Remove {
        /// Agent name
        name: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Open configuration in editor
    Edit,
}

#[derive(Subcommand)]
enum SkillsCommands {
    /// List installed skills
    List,
    /// Install skills to agent command directories
    Install,
    /// Remove skills from agent command directories
    Uninstall,
}

#[derive(Subcommand)]
enum ShellCommands {
    /// Show shell integration status
    Status,
    /// Install shell integration to .zshrc/.bashrc
    Install,
    /// Remove shell integration from .zshrc/.bashrc
    Uninstall,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record { agent, args } => cmd_record(&agent, &args),
        Commands::Status => cmd_status(),
        Commands::Cleanup { agent, older_than } => cmd_cleanup(agent.as_deref(), older_than),
        Commands::List { agent } => cmd_list(agent.as_deref()),
        Commands::Marker(cmd) => match cmd {
            MarkerCommands::Add { file, time, label } => cmd_marker_add(&file, time, &label),
            MarkerCommands::List { file } => cmd_marker_list(&file),
        },
        Commands::Agents(cmd) => match cmd {
            AgentCommands::List => cmd_agents_list(),
            AgentCommands::Add { name } => cmd_agents_add(&name),
            AgentCommands::Remove { name } => cmd_agents_remove(&name),
            AgentCommands::IsWrapped { name } => cmd_agents_is_wrapped(&name),
            AgentCommands::NoWrap(nowrap_cmd) => match nowrap_cmd {
                NoWrapCommands::List => cmd_agents_nowrap_list(),
                NoWrapCommands::Add { name } => cmd_agents_nowrap_add(&name),
                NoWrapCommands::Remove { name } => cmd_agents_nowrap_remove(&name),
            },
        },
        Commands::Config(cmd) => match cmd {
            ConfigCommands::Show => cmd_config_show(),
            ConfigCommands::Edit => cmd_config_edit(),
        },
        Commands::Skills(cmd) => match cmd {
            SkillsCommands::List => cmd_skills_list(),
            SkillsCommands::Install => cmd_skills_install(),
            SkillsCommands::Uninstall => cmd_skills_uninstall(),
        },
        Commands::Shell(cmd) => match cmd {
            ShellCommands::Status => cmd_shell_status(),
            ShellCommands::Install => cmd_shell_install(),
            ShellCommands::Uninstall => cmd_shell_uninstall(),
        },
    }
}

fn cmd_record(agent: &str, args: &[String]) -> Result<()> {
    let config = Config::load()?;

    if !config.is_agent_enabled(agent) {
        eprintln!("Warning: Agent '{}' is not in the configured list.", agent);
        eprintln!("Add it with: asr agents add {}", agent);
        eprintln!();
    }

    let mut recorder = Recorder::new(config);
    recorder.record(agent, args)
}

fn cmd_status() -> Result<()> {
    let config = Config::load()?;
    let storage = StorageManager::new(config);
    let stats = storage.get_stats()?;
    println!("{}", stats.summary());
    Ok(())
}

fn cmd_cleanup(agent_filter: Option<&str>, older_than: Option<u32>) -> Result<()> {
    let config = Config::load()?;
    let age_threshold = config.storage.age_threshold_days;
    let storage = StorageManager::new(config);

    // Get sessions, optionally filtered by agent
    let mut sessions = storage.list_sessions(agent_filter)?;

    // Apply older_than filter if specified
    if let Some(days) = older_than {
        sessions.retain(|s| s.age_days > days as i64);
    }

    if sessions.is_empty() {
        if agent_filter.is_some() || older_than.is_some() {
            println!("No sessions match the specified filters.");
        } else {
            println!("No sessions to clean up.");
        }
        return Ok(());
    }

    let stats = storage.get_stats()?;

    // Count old sessions (older than configured threshold)
    let old_count = sessions
        .iter()
        .filter(|s| s.age_days > age_threshold as i64)
        .count();

    // Print header with breakdown by agent
    println!("=== Agent Session Cleanup ===");
    println!(
        "Storage: {} ({:.1}% of disk)",
        stats.size_human(),
        stats.disk_percentage
    );

    // Show agent breakdown
    let agents_summary: Vec<String> = stats
        .sessions_by_agent
        .iter()
        .map(|(agent, count)| format!("{}: {}", agent, count))
        .collect();
    if !agents_summary.is_empty() {
        println!(
            "   Sessions: {} total ({})",
            stats.session_count,
            agents_summary.join(", ")
        );
    }
    println!();

    // Show filter info if applicable
    if let Some(agent) = agent_filter {
        println!("Filtered by agent: {}", agent);
    }
    if let Some(days) = older_than {
        println!("Filtered by age: > {} days", days);
    }
    if agent_filter.is_some() || older_than.is_some() {
        println!();
    }

    // Build session summary message
    let session_msg = if old_count > 0 {
        format!(
            "Found {} sessions ({} older than {} days - marked with *)",
            sessions.len(),
            old_count,
            age_threshold
        )
    } else {
        format!("Found {} sessions", sessions.len())
    };
    println!("{}", session_msg);
    println!();

    // Print formatted table header
    println!("  #  | Age   | Agent       | Size       | Filename");
    println!("-----+-------+-------------+------------+---------------------------");

    // Display up to 15 sessions in a formatted table
    for (i, session) in sessions.iter().take(15).enumerate() {
        let age_marker = if session.age_days > age_threshold as i64 {
            "*"
        } else {
            " "
        };
        println!(
            "{:>3}  | {:>3}d{} | {:11} | {:>10} | {}",
            i + 1,
            session.age_days,
            age_marker,
            truncate_string(&session.agent, 11),
            session.size_human(),
            session.filename
        );
    }

    if sessions.len() > 15 {
        println!("... and {} more sessions", sessions.len() - 15);
    }

    println!();

    // Build prompt with quick delete options
    let prompt = if old_count > 0 {
        format!(
            "Delete: [number], 'old' ({} sessions > {}d), 'all', or 0 to cancel: ",
            old_count, age_threshold
        )
    } else {
        "Delete: [number], 'all', or 0 to cancel: ".to_string()
    };
    print!("{}", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    // Parse input - could be number, 'old', or 'all'
    let to_delete: Vec<_> = if input == "0" || input.is_empty() {
        println!("No sessions deleted.");
        return Ok(());
    } else if input == "all" {
        sessions.clone()
    } else if input == "old" && old_count > 0 {
        sessions
            .iter()
            .filter(|s| s.age_days > age_threshold as i64)
            .cloned()
            .collect()
    } else if let Ok(count) = input.parse::<usize>() {
        if count > sessions.len() {
            println!("Invalid number. Maximum is {}.", sessions.len());
            return Ok(());
        }
        sessions.into_iter().take(count).collect()
    } else {
        println!("Invalid input. Use a number, 'old', 'all', or 0 to cancel.");
        return Ok(());
    };

    if to_delete.is_empty() {
        println!("No sessions to delete.");
        return Ok(());
    }

    // Calculate total size to be freed
    let total_size: u64 = to_delete.iter().map(|s| s.size).sum();

    println!();
    println!(
        "Will delete {} sessions ({}):",
        to_delete.len(),
        humansize::format_size(total_size, humansize::BINARY)
    );
    for session in to_delete.iter().take(10) {
        println!("  - {} ({})", session.filename, session.agent);
    }
    if to_delete.len() > 10 {
        println!("  ... and {} more", to_delete.len() - 10);
    }

    print!("\nConfirm? [y/N]: ");
    io::stdout().flush()?;

    let mut confirm = String::new();
    io::stdin().lock().read_line(&mut confirm)?;

    if confirm.trim().to_lowercase() == "y" {
        let freed = storage.delete_sessions(&to_delete)?;
        let new_stats = storage.get_stats()?;
        println!(
            "Deleted {} sessions (freed {}). New size: {}",
            to_delete.len(),
            humansize::format_size(freed, humansize::BINARY),
            new_stats.size_human()
        );
    } else {
        println!("Cancelled.");
    }

    Ok(())
}

/// Truncate a string to a maximum length, adding ellipsis if needed
fn truncate_string(s: &str, max_len: usize) -> String {
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

fn cmd_list(agent: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let storage = StorageManager::new(config);
    let mut sessions = storage.list_sessions(agent)?;

    if sessions.is_empty() {
        if let Some(agent_name) = agent {
            println!("No sessions found for agent '{}'.", agent_name);
        } else {
            println!("No sessions found.");
        }
        return Ok(());
    }

    // Reverse to show newest first
    sessions.reverse();

    // Print summary header
    if let Some(agent_name) = &agent {
        // Just show count for filtered view
        println!(
            "Sessions: {} (filtered by agent: {})",
            sessions.len(),
            agent_name
        );
    } else {
        // Show full summary with agent breakdown
        let stats = storage.get_stats()?;
        let mut agents: Vec<_> = stats.sessions_by_agent.iter().collect();
        agents.sort_by(|a, b| a.0.cmp(b.0));
        let agents_summary: Vec<String> = agents
            .iter()
            .map(|(agent, count)| format!("{}: {}", agent, count))
            .collect();
        if agents_summary.is_empty() {
            println!("Sessions: {} total", stats.session_count);
        } else {
            println!(
                "Sessions: {} total ({})",
                stats.session_count,
                agents_summary.join(", ")
            );
        }
    }
    println!();

    // Print table header
    println!("  #  | Age  | Agent       | Size       | Filename");
    println!("-----+------+-------------+------------+---------------------------");

    // Display sessions in formatted table
    for (i, session) in sessions.iter().enumerate() {
        println!(
            "{:>3}  | {:>3}d | {:11} | {:>10} | {}",
            i + 1,
            session.age_days,
            truncate_string(&session.agent, 11),
            session.size_human(),
            session.filename
        );
    }

    Ok(())
}

fn cmd_marker_add(file: &str, time: f64, label: &str) -> Result<()> {
    MarkerManager::add_marker(file, time, label)?;
    println!("Marker added at {:.1}s: \"{}\"", time, label);
    Ok(())
}

fn cmd_marker_list(file: &str) -> Result<()> {
    let markers = MarkerManager::list_markers(file)?;

    if markers.is_empty() {
        println!("No markers found in file.");
        return Ok(());
    }

    println!("Markers:");
    for marker in markers {
        println!("  {}", marker);
    }

    Ok(())
}

fn cmd_agents_list() -> Result<()> {
    let config = Config::load()?;

    if config.agents.enabled.is_empty() {
        println!("No agents configured.");
        return Ok(());
    }

    println!("Configured agents:");
    for agent in &config.agents.enabled {
        println!("  {}", agent);
    }

    Ok(())
}

fn cmd_agents_add(name: &str) -> Result<()> {
    let mut config = Config::load()?;

    if config.add_agent(name) {
        config.save()?;
        println!("Added agent: {}", name);
    } else {
        println!("Agent '{}' is already configured.", name);
    }

    Ok(())
}

fn cmd_agents_remove(name: &str) -> Result<()> {
    let mut config = Config::load()?;

    if config.remove_agent(name) {
        config.save()?;
        println!("Removed agent: {}", name);
    } else {
        println!("Agent '{}' was not configured.", name);
    }

    Ok(())
}

fn cmd_agents_is_wrapped(name: &str) -> Result<()> {
    let config = Config::load()?;

    if config.should_wrap_agent(name) {
        // Exit code 0 = should wrap
        std::process::exit(0);
    } else {
        // Exit code 1 = should not wrap
        std::process::exit(1);
    }
}

fn cmd_agents_nowrap_list() -> Result<()> {
    let config = Config::load()?;

    if config.agents.no_wrap.is_empty() {
        println!("No agents in no-wrap list. All enabled agents will be auto-wrapped.");
    } else {
        println!("Agents not auto-wrapped:");
        for agent in &config.agents.no_wrap {
            println!("  {}", agent);
        }
    }

    Ok(())
}

fn cmd_agents_nowrap_add(name: &str) -> Result<()> {
    let mut config = Config::load()?;

    if config.add_no_wrap(name) {
        config.save()?;
        println!(
            "Added '{}' to no-wrap list. It will not be auto-wrapped.",
            name
        );
    } else {
        println!("Agent '{}' is already in the no-wrap list.", name);
    }

    Ok(())
}

fn cmd_agents_nowrap_remove(name: &str) -> Result<()> {
    let mut config = Config::load()?;

    if config.remove_no_wrap(name) {
        config.save()?;
        println!(
            "Removed '{}' from no-wrap list. It will now be auto-wrapped.",
            name
        );
    } else {
        println!("Agent '{}' was not in the no-wrap list.", name);
    }

    Ok(())
}

fn cmd_config_show() -> Result<()> {
    let config = Config::load()?;
    let toml_str = toml::to_string_pretty(&config)?;
    println!("{}", toml_str);
    Ok(())
}

fn cmd_config_edit() -> Result<()> {
    let config_path = Config::config_path()?;

    // Ensure config exists
    if !config_path.exists() {
        let config = Config::default();
        config.save()?;
    }

    // Get editor from environment
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    println!("Opening {} with {}", config_path.display(), editor);

    std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))?;

    Ok(())
}

fn cmd_skills_list() -> Result<()> {
    let installed = asr::skills::list_installed_skills();

    if installed.is_empty() {
        println!("No skills installed.");
        println!();
        println!("Available skills:");
        for (name, _) in asr::skills::SKILLS {
            println!("  {}", name);
        }
        println!();
        println!("Run 'asr skills install' to install skills.");
        return Ok(());
    }

    println!("Installed skills:");
    for skill in &installed {
        let status = if skill.matches_embedded {
            "current"
        } else {
            "modified"
        };
        println!("  {} [{}]", skill.path.display(), status);
    }

    // Check for any directories without skills
    let dirs = asr::skills::skill_directories();
    let missing: Vec<_> = dirs
        .iter()
        .filter(|dir| !installed.iter().any(|s| s.path.starts_with(dir)))
        .collect();

    if !missing.is_empty() {
        println!();
        println!("Skills not installed in:");
        for dir in missing {
            println!("  {}", dir.display());
        }
        println!();
        println!("Run 'asr skills install' to install to all directories.");
    }

    Ok(())
}

fn cmd_skills_install() -> Result<()> {
    println!("Installing skills...");

    match asr::skills::install_skills() {
        Ok(installed) => {
            for path in &installed {
                println!("  Installed: {}", path.display());
            }
            println!();
            println!("Installed {} skill files.", installed.len());
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("Failed to install skills: {}", e)),
    }
}

fn cmd_skills_uninstall() -> Result<()> {
    println!("Removing skills...");

    match asr::skills::uninstall_skills() {
        Ok(removed) => {
            if removed.is_empty() {
                println!("No skills were installed.");
            } else {
                for path in &removed {
                    println!("  Removed: {}", path.display());
                }
                println!();
                println!("Removed {} skill files.", removed.len());
            }
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("Failed to remove skills: {}", e)),
    }
}

fn cmd_shell_status() -> Result<()> {
    let config = Config::load()?;
    let status = asr::shell::get_status(config.shell.auto_wrap);
    println!("{}", status.summary());
    Ok(())
}

fn cmd_shell_install() -> Result<()> {
    // Detect shell RC file
    let rc_file = asr::shell::detect_shell_rc()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    // Determine script path (use config dir)
    let script_path = asr::shell::default_script_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    // Install the shell script to config directory
    asr::shell::install_script(&script_path)
        .map_err(|e| anyhow::anyhow!("Failed to install shell script: {}", e))?;
    println!("Installed shell script: {}", script_path.display());

    // Install shell integration to RC file
    asr::shell::install(&rc_file, &script_path)
        .map_err(|e| anyhow::anyhow!("Failed to install shell integration: {}", e))?;
    println!("Installed shell integration: {}", rc_file.display());

    println!();
    println!("Shell integration installed successfully.");
    println!("Restart your shell or run: source {}", rc_file.display());

    Ok(())
}

fn cmd_shell_uninstall() -> Result<()> {
    // Find where shell integration is installed
    let rc_file = match asr::shell::find_installed_rc() {
        Some(rc) => rc,
        None => {
            println!("Shell integration is not installed.");
            return Ok(());
        }
    };

    // Remove from RC file
    let removed = asr::shell::uninstall(&rc_file)
        .map_err(|e| anyhow::anyhow!("Failed to remove shell integration: {}", e))?;

    if removed {
        println!("Removed shell integration from: {}", rc_file.display());

        // Extract the actual script path from RC file, fallback to default
        let script_path = asr::shell::extract_script_path(&rc_file)
            .ok()
            .flatten()
            .or_else(asr::shell::default_script_path);

        if let Some(script_path) = script_path {
            if script_path.exists() {
                std::fs::remove_file(&script_path)
                    .map_err(|e| anyhow::anyhow!("Failed to remove shell script: {}", e))?;
                println!("Removed shell script: {}", script_path.display());
            }
        }

        println!();
        println!("Shell integration removed successfully.");
        println!("Restart your shell to complete the removal.");
    } else {
        println!("Shell integration was not found in: {}", rc_file.display());
    }

    Ok(())
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

    #[test]
    fn cli_cleanup_parses_with_no_args() {
        let cli = Cli::try_parse_from(["asr", "cleanup"]).unwrap();
        match cli.command {
            Commands::Cleanup { agent, older_than } => {
                assert!(agent.is_none());
                assert!(older_than.is_none());
            }
            _ => panic!("Expected Cleanup command"),
        }
    }

    #[test]
    fn cli_cleanup_parses_with_agent_flag() {
        let cli = Cli::try_parse_from(["asr", "cleanup", "--agent", "claude"]).unwrap();
        match cli.command {
            Commands::Cleanup { agent, older_than } => {
                assert_eq!(agent, Some("claude".to_string()));
                assert!(older_than.is_none());
            }
            _ => panic!("Expected Cleanup command"),
        }
    }

    #[test]
    fn cli_cleanup_parses_with_older_than_flag() {
        let cli = Cli::try_parse_from(["asr", "cleanup", "--older-than", "30"]).unwrap();
        match cli.command {
            Commands::Cleanup { agent, older_than } => {
                assert!(agent.is_none());
                assert_eq!(older_than, Some(30));
            }
            _ => panic!("Expected Cleanup command"),
        }
    }

    #[test]
    fn cli_cleanup_parses_with_both_flags() {
        let cli = Cli::try_parse_from(["asr", "cleanup", "--agent", "codex", "--older-than", "60"])
            .unwrap();
        match cli.command {
            Commands::Cleanup { agent, older_than } => {
                assert_eq!(agent, Some("codex".to_string()));
                assert_eq!(older_than, Some(60));
            }
            _ => panic!("Expected Cleanup command"),
        }
    }

    #[test]
    fn cli_skills_list_parses() {
        let cli = Cli::try_parse_from(["asr", "skills", "list"]).unwrap();
        match cli.command {
            Commands::Skills(SkillsCommands::List) => {}
            _ => panic!("Expected Skills List command"),
        }
    }

    #[test]
    fn cli_skills_install_parses() {
        let cli = Cli::try_parse_from(["asr", "skills", "install"]).unwrap();
        match cli.command {
            Commands::Skills(SkillsCommands::Install) => {}
            _ => panic!("Expected Skills Install command"),
        }
    }

    #[test]
    fn cli_skills_uninstall_parses() {
        let cli = Cli::try_parse_from(["asr", "skills", "uninstall"]).unwrap();
        match cli.command {
            Commands::Skills(SkillsCommands::Uninstall) => {}
            _ => panic!("Expected Skills Uninstall command"),
        }
    }

    #[test]
    fn cli_shell_status_parses() {
        let cli = Cli::try_parse_from(["asr", "shell", "status"]).unwrap();
        match cli.command {
            Commands::Shell(ShellCommands::Status) => {}
            _ => panic!("Expected Shell Status command"),
        }
    }

    #[test]
    fn cli_shell_install_parses() {
        let cli = Cli::try_parse_from(["asr", "shell", "install"]).unwrap();
        match cli.command {
            Commands::Shell(ShellCommands::Install) => {}
            _ => panic!("Expected Shell Install command"),
        }
    }

    #[test]
    fn cli_shell_uninstall_parses() {
        let cli = Cli::try_parse_from(["asr", "shell", "uninstall"]).unwrap();
        match cli.command {
            Commands::Shell(ShellCommands::Uninstall) => {}
            _ => panic!("Expected Shell Uninstall command"),
        }
    }
}
