//! Nested subcommand enums (marker, agents, no-wrap, config, shell).

use clap::Subcommand;

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
    /// Add missing fields to config file
    #[command(long_about = "Add missing fields to your config file.

Scans your config file and adds any fields that exist in the current
version but are missing from your file. Preserves your existing values,
comments, and formatting.

This is useful after upgrading AGR to a new version that introduces
new configuration options. The command shows a preview of changes
and asks for confirmation before writing.

EXAMPLES:
    agr config migrate              Interactive mode (shows preview, asks confirmation)
    agr config migrate --yes        Apply changes without confirmation (for scripts/CI)")]
    Migrate {
        /// Apply changes without confirmation prompt
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
    },
    /// Reset configuration to defaults
    #[command(long_about = "Reset configuration to default values.

Backs up the current config file (if it exists) to config.toml.bak,
then writes a fresh default configuration.

EXAMPLES:
    agr config reset               Interactive mode (asks confirmation)
    agr config reset --yes         Reset without confirmation (for scripts/CI)")]
    Reset {
        /// Apply reset without confirmation prompt
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
    },
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

#[derive(Subcommand)]
pub enum SyncCommands {
    /// Show what push and pull would do, per session
    #[command(long_about = "Show what `agr sync push` and `agr sync pull` would do.

Read only, never prompts. Needs the network to list the remote sessions.

EXAMPLE:
    agr sync status")]
    Status,
    /// Upload new and changed sessions (encrypted)
    #[command(
        long_about = "Upload new and changed sessions, encrypted with your sync key.

The first push that creates gists shows a privacy notice and asks for
confirmation. Without a terminal it needs --yes, otherwise nothing changes.
Conflicts are skipped unless --overwrite is given (local wins).

EXAMPLES:
    agr sync push                    Push everything that changed
    agr sync push claude/a.cast.gz   Push one session
    agr sync push --dry-run          Show what would be pushed"
    )]
    Push {
        /// Sessions to push (default: all)
        #[arg(help = "Sessions to push (default: all)")]
        sessions: Vec<String>,
        /// Show what would happen, change nothing
        #[arg(long, help = "Show what would happen, change nothing")]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip the confirmation prompt")]
        yes: bool,
        /// Resolve conflicts with the local session
        #[arg(long, help = "Resolve conflicts with the local session")]
        overwrite: bool,
        /// Internal: push after a recording, never prompts
        #[arg(long, hide = true)]
        auto: bool,
    },
    /// Download new and changed sessions
    #[command(
        long_about = "Download remote sessions, decrypt and verify them before writing.

Needs your sync key (`agr sync key import` on a second machine).
Conflicts are skipped unless --overwrite is given (remote wins, the
replaced file is kept as a hidden .bak).

EXAMPLES:
    agr sync pull                    Pull everything that is new or changed
    agr sync pull --dry-run          Show what would be pulled"
    )]
    Pull {
        /// Sessions to pull (default: all)
        #[arg(help = "Sessions to pull (default: all)")]
        sessions: Vec<String>,
        /// Show what would happen, change nothing
        #[arg(long, help = "Show what would happen, change nothing")]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip the confirmation prompt")]
        yes: bool,
        /// Resolve conflicts with the remote session
        #[arg(long, help = "Resolve conflicts with the remote session")]
        overwrite: bool,
    },
    /// Remove sessions from the remote (the local file stays)
    #[command(long_about = "Remove the remote copy of a session and its index entry.

The local file is never touched.

EXAMPLE:
    agr sync delete claude/a.cast.gz")]
    Delete {
        /// Sessions to delete from the remote
        #[arg(required = true, help = "Sessions to delete from the remote")]
        sessions: Vec<String>,
        /// Show what would happen, change nothing
        #[arg(long, help = "Show what would happen, change nothing")]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip the confirmation prompt")]
        yes: bool,
    },
    /// Export or import the sync key
    #[command(
        subcommand,
        long_about = "Export or import the sync key.

SUBCOMMANDS:
    agr sync key export              Print the recovery phrase
    agr sync key import [PHRASE]     Install a recovery phrase (stdin when omitted)"
    )]
    Key(SyncKeyCommands),
}

#[derive(Subcommand)]
pub enum SyncKeyCommands {
    /// Print the recovery phrase of the sync key
    #[command(long_about = "Print the recovery phrase of the sync key.

Store it somewhere safe. Losing every copy of the key makes synced
sessions unrecoverable.

EXAMPLE:
    agr sync key export")]
    Export,
    /// Import a recovery phrase on this machine
    #[command(long_about = "Import a recovery phrase (argument or stdin).

Refuses to replace a different existing key without --force.
Prefer stdin, an argument ends up in your shell history.

EXAMPLES:
    agr sync key import              Read the phrase from stdin
    agr sync key import agr1-XXXXX-...")]
    Import {
        /// Recovery phrase (read from stdin when omitted, stdin avoids shell history)
        #[arg(help = "Recovery phrase (stdin when omitted, avoids shell history)")]
        phrase: Option<String>,
        /// Replace a different existing key
        #[arg(long, help = "Replace a different existing key")]
        force: bool,
    },
}
