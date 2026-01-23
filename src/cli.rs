//! CLI definitions for AGR
//!
//! This module contains the clap CLI structure definitions, separated from main.rs
//! so they can be accessed by xtask for documentation generation (man pages, markdown, wiki).

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Parser, Subcommand};
use clap_complete::Shell as CompletionShell;

/// Build clap styles using our theme colors.
///
/// Maps theme colors to clap's styling system for consistent CLI appearance.
/// - Green: headers, usage, command names (accent color)
/// - White: descriptions, placeholders (renders as light gray on dark terminals)
pub fn build_cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Green.on_default())
        .placeholder(AnsiColor::White.on_default()) // Light gray for descriptions
        .valid(AnsiColor::White.on_default()) // Light gray for valid values
        .invalid(AnsiColor::Red.on_default())
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
}

#[derive(Parser)]
#[command(name = "agr")]
#[command(
    about = "[ Agent Session Recorder ] - auto-record agent sessions and handle the recordings with AI!"
)]
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
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
    agr record gemini -- chat        Start gemini in chat mode")]
    Record {
        /// Agent name (e.g., claude, codex, gemini)
        #[arg(help = "Agent name (e.g., claude, codex, gemini)")]
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
    #[command(
        visible_alias = "ls",
        long_about = "List all recorded sessions with details.

Shows sessions sorted by date (newest first) with agent name,
age, file size, and filename.

EXAMPLES:
    agr list                List all sessions
    agr ls                  Same as 'agr list' (alias)
    agr list claude         List only Claude sessions
    agr list codex          List only Codex sessions"
    )]
    List {
        /// Filter by agent name
        #[arg(help = "Filter sessions by agent name")]
        agent: Option<String>,
    },

    /// Analyze a recording with AI
    #[command(long_about = "Analyze a recording file using an AI agent.

The analyzer reads the cast file, identifies key moments (errors, decisions,
milestones), and adds markers using 'agr marker add'. This is the same
analysis that runs automatically when auto_analyze is enabled.

The default agent is configured in ~/.config/agr/config.toml under
[recording].analysis_agent. Use --agent to override for a single run.

EXAMPLES:
    agr analyze session.cast              Analyze with default agent
    agr analyze session.cast --agent codex    Override agent for this run

SUPPORTED AGENTS:
    claude      Claude Code CLI
    codex       OpenAI Codex CLI
    gemini  Google Gemini CLI")]
    Analyze {
        /// Path to the .cast file to analyze
        #[arg(help = "Path to the .cast recording file")]
        file: String,
        /// Override the configured analysis agent
        #[arg(long, short, help = "Agent to use (overrides config)")]
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

    /// Generate shell completions (internal use)
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for
        #[arg(long, value_enum)]
        shell: Option<CompletionShell>,

        /// List cast files for completion (outputs agent/filename.cast format)
        #[arg(long)]
        files: bool,

        /// Filter prefix for file listing
        #[arg(default_value = "")]
        prefix: String,
    },
}

#[derive(Subcommand)]
pub enum MarkerCommands {
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
pub enum AgentCommands {
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
pub enum NoWrapCommands {
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
pub enum ConfigCommands {
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
pub enum ShellCommands {
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
