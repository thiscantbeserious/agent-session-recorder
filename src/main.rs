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
    Cleanup,

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
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Show current configuration
    Show,
    /// Open configuration in editor
    Edit,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Record { agent, args } => cmd_record(&agent, &args),
        Commands::Status => cmd_status(),
        Commands::Cleanup => cmd_cleanup(),
        Commands::List { agent } => cmd_list(agent.as_deref()),
        Commands::Marker(cmd) => match cmd {
            MarkerCommands::Add { file, time, label } => cmd_marker_add(&file, time, &label),
            MarkerCommands::List { file } => cmd_marker_list(&file),
        },
        Commands::Agents(cmd) => match cmd {
            AgentCommands::List => cmd_agents_list(),
            AgentCommands::Add { name } => cmd_agents_add(&name),
            AgentCommands::Remove { name } => cmd_agents_remove(&name),
        },
        Commands::Config(cmd) => match cmd {
            ConfigCommands::Show => cmd_config_show(),
            ConfigCommands::Edit => cmd_config_edit(),
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

fn cmd_cleanup() -> Result<()> {
    let config = Config::load()?;
    let storage = StorageManager::new(config);
    let sessions = storage.list_sessions(None)?;

    if sessions.is_empty() {
        println!("No sessions to clean up.");
        return Ok(());
    }

    let stats = storage.get_stats()?;

    println!("=== Agent Session Cleanup ===");
    println!("Storage: {} ({:.1}% of disk)", stats.size_human(), stats.disk_percentage);
    println!();
    println!("Found {} sessions. Oldest 10:", sessions.len());

    for (i, session) in sessions.iter().take(10).enumerate() {
        println!(
            "  {}) {} ({}, {}, {} days)",
            i + 1,
            session.filename,
            session.agent,
            session.size_human(),
            session.age_days
        );
    }

    println!();
    print!("How many oldest to delete? [0-{}]: ", sessions.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let count: usize = input.trim().parse().unwrap_or(0);

    if count == 0 {
        println!("No sessions deleted.");
        return Ok(());
    }

    let to_delete: Vec<_> = sessions.into_iter().take(count).collect();

    println!();
    println!("Will delete:");
    for session in &to_delete {
        println!("  - {}", session.filename);
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

fn cmd_list(agent: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let storage = StorageManager::new(config);
    let sessions = storage.list_sessions(agent)?;

    if sessions.is_empty() {
        if let Some(agent_name) = agent {
            println!("No sessions found for agent '{}'.", agent_name);
        } else {
            println!("No sessions found.");
        }
        return Ok(());
    }

    println!("Sessions:");
    for session in sessions {
        println!(
            "  {} ({}, {}, {} days old)",
            session.filename,
            session.agent,
            session.size_human(),
            session.age_days
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
