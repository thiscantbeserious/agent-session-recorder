//! Agent Session Recorder (AGR) - CLI entry point

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::io::{self, BufRead, Write};

use agr::{Config, MarkerManager, Recorder, StorageManager};

#[derive(Parser)]
#[command(name = "agr")]
#[command(about = "Agent Session Recorder - Record AI agent terminal sessions")]
#[command(
    long_about = "Agent Session Recorder (AGR) - Record AI agent terminal sessions with asciinema.

AGR automatically records your AI coding agent sessions (Claude, Codex, Gemini, etc.)
to ~/recorded_agent_sessions/ in asciicast v3 format. Recordings can be played back
with asciinema, auto-analyzed by AI agents, and annotated with markers.

QUICK START:
    agr record claude              Record a Claude session
    agr status                     Check storage usage
    agr list                       List all recordings
    agr cleanup                    Clean up old recordings

SHELL INTEGRATION:
    agr shell install              Auto-record configured agents
    agr agents add claude          Add agent to auto-record list

For more information, see: https://github.com/thiscantbeserious/agent-session-recorder"
)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start recording a session
    #[command(long_about = "Start recording an AI agent session with asciinema.

The recording is saved to ~/recorded_agent_sessions/<agent>/<timestamp>.cast
in asciicast v3 format. When the session ends, you can optionally rename
the recording for easier identification.

EXAMPLES:
    agr record claude                    Record a Claude Code session
    agr record codex                     Record an OpenAI Codex session
    agr record claude --name my-session  Record with a specific filename
    agr record claude -- --help          Pass --help flag to claude
    agr record gemini-cli -- chat        Start gemini-cli in chat mode")]
    Record {
        /// Agent name (e.g., claude, codex, gemini-cli)
        #[arg(help = "Agent name (e.g., claude, codex, gemini-cli)")]
        agent: String,
        /// Optional session name (skips rename prompt)
        #[arg(long, short, help = "Session name (skips rename prompt)")]
        name: Option<String>,
        /// Arguments to pass to the agent command
        #[arg(last = true, help = "Arguments to pass to the agent (after --)")]
        args: Vec<String>,
    },

    /// Show storage statistics
    #[command(long_about = "Display storage statistics for recorded sessions.

Shows total size, disk usage percentage, session count by agent,
and age of the oldest recording.

EXAMPLE:
    agr status

OUTPUT:
    Agent Sessions: 1.2 GB (0.5% of disk)
       Sessions: 23 total (claude: 15, codex: 8)
       Oldest: 2025-01-01 (20 days ago)")]
    Status,

    /// Interactive cleanup of old sessions
    #[command(
        long_about = "Interactively delete old session recordings to free up disk space.

Displays a list of sessions sorted by age and lets you choose how many
to delete. Supports filtering by agent and age. Sessions older than
the configured threshold (default: 30 days) are marked with *.

EXAMPLES:
    agr cleanup                          Interactive cleanup of all sessions
    agr cleanup --agent claude           Only show Claude sessions
    agr cleanup --older-than 60          Only show sessions older than 60 days
    agr cleanup --agent codex --older-than 30

INTERACTIVE OPTIONS:
    [number]    Delete the N oldest sessions
    'old'       Delete all sessions older than threshold
    'all'       Delete all matching sessions
    0           Cancel without deleting"
    )]
    Cleanup {
        /// Filter sessions by agent name
        #[arg(long, help = "Only show sessions from this agent")]
        agent: Option<String>,
        /// Only show sessions older than N days
        #[arg(long, help = "Only show sessions older than N days")]
        older_than: Option<u32>,
    },

    /// List recorded sessions
    #[command(long_about = "List all recorded sessions with details.

Shows sessions sorted by date (newest first) with agent name,
age, file size, and filename.

EXAMPLES:
    agr list                List all sessions
    agr list claude         List only Claude sessions
    agr list codex          List only Codex sessions")]
    List {
        /// Filter by agent name
        #[arg(help = "Filter sessions by agent name")]
        agent: Option<String>,
    },

    /// Manage markers in cast files
    #[command(
        subcommand,
        long_about = "Add and list markers in asciicast recording files.

Markers are annotations at specific timestamps in a recording,
useful for highlighting key moments like errors, decisions, or
milestones. Markers use the native asciicast v3 marker format.

EXAMPLES:
    agr marker add session.cast 45.2 \"Build failed\"
    agr marker add session.cast 120.5 \"Deployment complete\"
    agr marker list session.cast"
    )]
    Marker(MarkerCommands),

    /// Manage configured agents
    #[command(
        subcommand,
        long_about = "Manage the list of AI agents that AGR knows about.

Configured agents are used by shell integration to automatically
record sessions. You can also control which agents are auto-wrapped
using the no-wrap subcommand.

EXAMPLES:
    agr agents list                  Show configured agents
    agr agents add claude            Add claude to the list
    agr agents remove codex          Remove codex from the list
    agr agents no-wrap add claude    Disable auto-wrap for claude"
    )]
    Agents(AgentCommands),

    /// Configuration management
    #[command(
        subcommand,
        long_about = "View and edit the AGR configuration file.

Configuration is stored in ~/.config/agr/config.toml and includes
storage settings, agent list, shell integration options, and more.

EXAMPLES:
    agr config show          Display current configuration
    agr config edit          Open config in $EDITOR"
    )]
    Config(ConfigCommands),

    /// Manage shell integration
    #[command(
        subcommand,
        long_about = "Manage automatic session recording via shell integration.

Shell integration adds wrapper functions to your shell that automatically
record sessions when you run configured agents. It modifies your .zshrc
or .bashrc with a clearly marked section.

EXAMPLES:
    agr shell status         Check if shell integration is installed
    agr shell install        Install shell integration
    agr shell uninstall      Remove shell integration

After installing, restart your shell or run: source ~/.zshrc"
    )]
    Shell(ShellCommands),
}

#[derive(Subcommand)]
enum MarkerCommands {
    /// Add a marker to a cast file at a specific timestamp
    #[command(long_about = "Add a marker to a cast file at a specific timestamp.

Markers are injected into the asciicast file using the native v3 marker
format. The timestamp is cumulative seconds from the start of the recording.

EXAMPLE:
    agr marker add ~/recorded_agent_sessions/claude/session.cast 45.2 \"Build error\"")]
    Add {
        /// Path to the .cast file
        #[arg(help = "Path to the .cast recording file")]
        file: String,
        /// Timestamp in seconds from start of recording
        #[arg(help = "Timestamp in seconds (e.g., 45.2)")]
        time: f64,
        /// Marker label/description
        #[arg(help = "Description of the marker (e.g., \"Build failed\")")]
        label: String,
    },
    /// List all markers in a cast file
    #[command(
        long_about = "List all markers in a cast file with their timestamps and labels.

EXAMPLE:
    agr marker list ~/recorded_agent_sessions/claude/session.cast

OUTPUT:
    Markers:
      [45.2s] Build error
      [120.5s] Deployment complete"
    )]
    List {
        /// Path to the .cast file
        #[arg(help = "Path to the .cast recording file")]
        file: String,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List all configured agents
    #[command(long_about = "List all agents configured for recording.

These agents can be auto-recorded when shell integration is enabled.")]
    List,
    /// Add an agent to the configuration
    #[command(long_about = "Add an agent to the configured list.

Once added, the agent can be auto-recorded via shell integration.

EXAMPLE:
    agr agents add claude
    agr agents add my-custom-agent")]
    Add {
        /// Agent name to add
        #[arg(help = "Name of the agent (e.g., claude, codex)")]
        name: String,
    },
    /// Remove an agent from the configuration
    #[command(long_about = "Remove an agent from the configured list.

The agent will no longer be auto-recorded via shell integration.

EXAMPLE:
    agr agents remove codex")]
    Remove {
        /// Agent name to remove
        #[arg(help = "Name of the agent to remove")]
        name: String,
    },
    /// Check if an agent should be wrapped (used by shell integration)
    #[command(
        name = "is-wrapped",
        long_about = "Check if an agent should be auto-wrapped by shell integration.

Returns exit code 0 if the agent should be wrapped, 1 if not.
Used internally by the shell integration script.

EXAMPLE:
    agr agents is-wrapped claude && echo \"Should wrap\""
    )]
    IsWrapped {
        /// Agent name to check
        #[arg(help = "Name of the agent to check")]
        name: String,
    },
    /// Manage agents that should not be auto-wrapped
    #[command(
        subcommand,
        long_about = "Manage the no-wrap list for agents that should not be auto-recorded.

Agents on this list will not be automatically wrapped by shell integration,
even if they are in the configured agents list. Useful for temporarily
disabling recording for specific agents."
    )]
    NoWrap(NoWrapCommands),
}

#[derive(Subcommand)]
enum NoWrapCommands {
    /// List agents that are excluded from auto-wrapping
    #[command(long_about = "List all agents on the no-wrap list.

These agents will not be auto-recorded even with shell integration enabled.")]
    List,
    /// Add an agent to the no-wrap list (disable auto-recording)
    #[command(long_about = "Add an agent to the no-wrap list.

The agent will not be auto-recorded by shell integration.

EXAMPLE:
    agr agents no-wrap add claude")]
    Add {
        /// Agent name to exclude from auto-wrapping
        #[arg(help = "Name of the agent to exclude")]
        name: String,
    },
    /// Remove an agent from the no-wrap list (re-enable auto-recording)
    #[command(long_about = "Remove an agent from the no-wrap list.

The agent will be auto-recorded again by shell integration.

EXAMPLE:
    agr agents no-wrap remove claude")]
    Remove {
        /// Agent name to re-enable for auto-wrapping
        #[arg(help = "Name of the agent to re-enable")]
        name: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration as TOML
    #[command(long_about = "Display the current configuration in TOML format.

Shows all settings including storage paths, agent list, shell options,
and recording preferences.

EXAMPLE:
    agr config show")]
    Show,
    /// Open configuration file in your default editor
    #[command(long_about = "Open the configuration file in your default editor.

Uses the $EDITOR environment variable (defaults to 'vi').
Config file location: ~/.config/agr/config.toml

EXAMPLE:
    agr config edit
    EDITOR=nano agr config edit")]
    Edit,
}

#[derive(Subcommand)]
enum ShellCommands {
    /// Show shell integration status
    #[command(long_about = "Show the current status of shell integration.

Displays whether shell integration is installed, which RC file
is configured, and whether auto-wrap is enabled.

EXAMPLE:
    agr shell status")]
    Status,
    /// Install shell integration to .zshrc/.bashrc
    #[command(
        long_about = "Install shell integration for automatic session recording.

Adds a clearly marked section to your .zshrc (or .bashrc) that
sources the AGR shell script. This creates wrapper functions for
configured agents that automatically record sessions.

After installation, restart your shell or run:
    source ~/.zshrc

EXAMPLE:
    agr shell install"
    )]
    Install,
    /// Remove shell integration from .zshrc/.bashrc
    #[command(long_about = "Remove shell integration from your shell configuration.

Removes the AGR section from your .zshrc/.bashrc and deletes
the shell script. Restart your shell after uninstalling.

EXAMPLE:
    agr shell uninstall")]
    Uninstall,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record { agent, name, args } => cmd_record(&agent, name.as_deref(), &args),
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
        Commands::Shell(cmd) => match cmd {
            ShellCommands::Status => cmd_shell_status(),
            ShellCommands::Install => cmd_shell_install(),
            ShellCommands::Uninstall => cmd_shell_uninstall(),
        },
    }
}

fn cmd_record(agent: &str, name: Option<&str>, args: &[String]) -> Result<()> {
    let config = Config::load()?;

    if !config.is_agent_enabled(agent) {
        eprintln!("Warning: Agent '{}' is not in the configured list.", agent);
        eprintln!("Add it with: agr agents add {}", agent);
        eprintln!();
    }

    let mut recorder = Recorder::new(config);
    recorder.record(agent, name, args)
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

fn cmd_shell_status() -> Result<()> {
    let config = Config::load()?;
    let status = agr::shell::get_status(config.shell.auto_wrap);
    println!("{}", status.summary());
    Ok(())
}

fn cmd_shell_install() -> Result<()> {
    // Detect shell RC file
    let rc_file = agr::shell::detect_shell_rc()
        .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

    // Determine script path (use config dir)
    let script_path = agr::shell::default_script_path()
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;

    // Install the shell script to config directory
    agr::shell::install_script(&script_path)
        .map_err(|e| anyhow::anyhow!("Failed to install shell script: {}", e))?;
    println!("Installed shell script: {}", script_path.display());

    // Install shell integration to RC file
    agr::shell::install(&rc_file, &script_path)
        .map_err(|e| anyhow::anyhow!("Failed to install shell integration: {}", e))?;
    println!("Installed shell integration: {}", rc_file.display());

    println!();
    println!("Shell integration installed successfully.");
    println!("Restart your shell or run: source {}", rc_file.display());

    Ok(())
}

fn cmd_shell_uninstall() -> Result<()> {
    // Find where shell integration is installed
    let rc_file = match agr::shell::find_installed_rc() {
        Some(rc) => rc,
        None => {
            println!("Shell integration is not installed.");
            return Ok(());
        }
    };

    // Remove from RC file
    let removed = agr::shell::uninstall(&rc_file)
        .map_err(|e| anyhow::anyhow!("Failed to remove shell integration: {}", e))?;

    if removed {
        println!("Removed shell integration from: {}", rc_file.display());

        // Extract the actual script path from RC file, fallback to default
        let script_path = agr::shell::extract_script_path(&rc_file)
            .ok()
            .flatten()
            .or_else(agr::shell::default_script_path);

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
        let cli = Cli::try_parse_from(["agr", "cleanup"]).unwrap();
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
        let cli = Cli::try_parse_from(["agr", "cleanup", "--agent", "claude"]).unwrap();
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
        let cli = Cli::try_parse_from(["agr", "cleanup", "--older-than", "30"]).unwrap();
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
        let cli = Cli::try_parse_from(["agr", "cleanup", "--agent", "codex", "--older-than", "60"])
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
    fn cli_shell_status_parses() {
        let cli = Cli::try_parse_from(["agr", "shell", "status"]).unwrap();
        match cli.command {
            Commands::Shell(ShellCommands::Status) => {}
            _ => panic!("Expected Shell Status command"),
        }
    }

    #[test]
    fn cli_shell_install_parses() {
        let cli = Cli::try_parse_from(["agr", "shell", "install"]).unwrap();
        match cli.command {
            Commands::Shell(ShellCommands::Install) => {}
            _ => panic!("Expected Shell Install command"),
        }
    }

    #[test]
    fn cli_shell_uninstall_parses() {
        let cli = Cli::try_parse_from(["agr", "shell", "uninstall"]).unwrap();
        match cli.command {
            Commands::Shell(ShellCommands::Uninstall) => {}
            _ => panic!("Expected Shell Uninstall command"),
        }
    }
}
