//! CLI definitions for AGR
//!
//! This module contains the clap CLI structure definitions, separated from main.rs
//! so they can be accessed by xtask for documentation generation (man pages, markdown, wiki).

mod styles;
mod subcommands;

pub use styles::build_cli_styles;
pub use subcommands::{
    AgentCommands, ConfigCommands, MarkerCommands, NoWrapCommands, ShellCommands,
};

use clap::{Parser, Subcommand};
use clap_complete::Shell as CompletionShell;

#[derive(Parser)]
#[command(name = "agr")]
#[command(about = "[ Agent Session Recorder ] - Record, replay, and understand AI agent sessions.")]
#[command(
    long_about = "[ Agent Session Recorder ] - Record, replay, and understand AI agent sessions.

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

Shows the size on disk, the uncompressed size, disk usage percentage,
session count by agent and age of the oldest recording. When plain .cast
sessions are left, a last line shows what `agr compress` would save.

EXAMPLE:
    agr status

OUTPUT:
    Agent Sessions: 1.2 GiB on disk, 6.8 GiB uncompressed (0.50% of disk)
       Sessions: 23 total (claude: 15, codex: 8)
       Oldest: 2025-01-01 (20 days ago)
    3 plain, ~410.00 MiB saved by agr compress")]
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

    /// Compress plain sessions to .cast.gz
    #[command(long_about = "Compress existing plain .cast sessions to .cast.gz.

Shows a preview with the number of sessions, current size and projected
size, then asks for confirmation. Sessions that are currently being
recorded are skipped. Without a TTY nothing is prompted and nothing is
rewritten, use --yes to apply.

EXAMPLES:
    agr compress                    Preview and ask for confirmation
    agr compress --agent claude     Only compress Claude sessions
    agr compress --yes              Compress without confirmation
    agr compress --dry-run          Only show the preview, change nothing

Ctrl+C stops after the current file and prints what was done so far.")]
    Compress {
        /// Filter sessions by agent name
        #[arg(long, help = "Only compress sessions from this agent")]
        agent: Option<String>,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip confirmation prompt")]
        yes: bool,
        /// Only print the preview
        #[arg(long, help = "Only show the preview, never prompt or change anything")]
        dry_run: bool,
    },

    /// Analyze a recording with AI
    #[command(long_about = "Analyze a recording file using an AI agent.

The analyzer reads the cast file, extracts meaningful content (removing ANSI
codes and noise), and uses AI to identify key engineering moments. Markers
are added directly to the file using the native asciicast v3 format.

For large files, analysis is parallelized across multiple chunks, with
automatic retry and rate limit handling.

The default agent is configured in ~/.config/agr/config.toml under
[analysis].agent. Use --agent to override for a single run.

EXAMPLES:
    agr analyze session.cast                     Analyze with default agent
    agr analyze session.cast --agent codex       Use Codex instead
    agr analyze session.cast --workers 4         Use 4 parallel workers
    agr analyze session.cast --no-parallel       Sequential mode
    agr analyze session.cast --timeout 180       3 minute timeout per chunk

SUPPORTED AGENTS:
    claude      Claude Code CLI (default)
    codex       OpenAI Codex CLI
    gemini      Google Gemini CLI")]
    Analyze {
        /// Path to the .cast file to analyze
        #[arg(help = "Path to the .cast recording file")]
        file: String,
        /// Override the configured analysis agent
        #[arg(long, short, help = "Agent to use: claude, codex, gemini")]
        agent: Option<String>,
        /// Number of parallel workers (default: auto-scale based on content)
        #[arg(long, short, help = "Number of parallel workers")]
        workers: Option<usize>,
        /// Timeout per chunk in seconds (default: 120)
        #[arg(long, short, help = "Timeout per chunk in seconds")]
        timeout: Option<u64>,
        /// Disable parallel processing (analyze sequentially)
        #[arg(long, help = "Disable parallel processing")]
        no_parallel: bool,
        /// Auto-curate markers without prompting (reduces to 8-12 best markers)
        #[arg(long, help = "Auto-curate to 8-12 markers without prompting")]
        curate: bool,
        /// Debug mode: required for --output to work
        #[arg(long, help = "Enable debug mode (required for --output)")]
        debug: bool,
        /// Save cleaned content and exit (requires --debug). Optionally specify filename.
        #[arg(
            long,
            short,
            value_name = "FILE",
            num_args = 0..=1,
            default_missing_value = "",
            require_equals = true,
            help = "Save cleaned content and exit (optionally specify filename)"
        )]
        output: Option<String>,
        /// Skip JSON schema enforcement for faster analysis (less reliable)
        #[arg(long, help = "Skip JSON schema enforcement (faster but less reliable)")]
        fast: bool,
        /// Wait for keypress before exiting (used by TUI)
        #[arg(long, hide = true)]
        wait: bool,
    },

    /// Play a recording with the native player
    #[command(long_about = "Play an asciicast recording using the native player.

The native player supports seeking, speed control, and marker navigation.
Recordings can be specified by absolute path, short format (agent/file.cast),
or just filename (fuzzy matches across all agents).

EXAMPLES:
    agr play session.cast                 Play by filename (fuzzy match)
    agr play claude/session.cast          Play using short format
    agr play /path/to/session.cast        Play by absolute path

PLAYER CONTROLS:
    q, Esc      Quit
    Space       Pause/resume
    +/-         Adjust playback speed
    <, > or ,, .  Seek backward/forward 5s
    m           Jump to next marker
    ?           Show help overlay")]
    Play {
        /// Path to the .cast file to play
        #[arg(help = "Path to the .cast recording file")]
        file: String,
    },

    /// Copy a recording to the clipboard
    #[command(long_about = "Copy a recording file to the system clipboard.

On macOS, the file is copied as a file reference, allowing direct paste
into Slack, email, or other applications as an attachment. On Linux,
the file content is copied as text (file copy not supported).

Recordings can be specified by absolute path, short format (agent/file.cast),
or just filename (fuzzy matches across all agents).

EXAMPLES:
    agr copy session.cast                 Copy by filename (fuzzy match)
    agr copy claude/session.cast          Copy using short format
    agr copy /path/to/session.cast        Copy by absolute path")]
    Copy {
        /// Path to the .cast file to copy
        #[arg(help = "Path to the .cast recording file")]
        file: String,
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

    /// Optimize asciicast recordings (removes silence)
    #[command(long_about = "Optimize asciicast recording files by removing silence.

Optimization modifies the timing of recordings by capping long pauses
at a configurable threshold.

THRESHOLD RESOLUTION:
    1. CLI argument (explicit user intent)
    2. Header's idle_time_limit (recording author's intent)
    3. Default: 2.0 seconds

EXAMPLES:
    agr optimize --remove-silence session.cast
        Use header's idle_time_limit or default 2.0s threshold

    agr optimize --remove-silence=1.5 session.cast
        Use explicit 1.5s threshold (note: requires = for value)

    agr optimize --remove-silence --output fast.cast session.cast
        Write to separate file, preserving original")]
    Optimize {
        /// Remove silence by capping intervals at threshold
        #[arg(
            long = "remove-silence",
            value_name = "SECONDS",
            num_args = 0..=1,
            default_missing_value = "",
            require_equals = true,
            help = "Cap intervals at threshold (default: header or 2.0s)"
        )]
        remove_silence: Option<String>,

        /// Write output to a separate file instead of modifying in-place
        #[arg(long, short, value_name = "FILE", help = "Output file path")]
        output: Option<String>,

        /// Path to the .cast file to optimize
        #[arg(help = "Path to the .cast recording file")]
        file: String,
    },

    /// Generate shell completions (internal use)
    #[command(hide = true)]
    Completions {
        /// Shell to generate completions for (clap native)
        #[arg(long, value_enum)]
        shell: Option<CompletionShell>,

        /// Output shell initialization code with embedded completions
        #[arg(long, value_enum)]
        shell_init: Option<CompletionShell>,

        /// Output uncompressed shell code for debugging
        #[arg(long)]
        debug: bool,

        /// List cast files for completion (outputs agent/filename.cast format)
        #[arg(long)]
        files: bool,

        /// Limit number of files returned
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Filter prefix for file listing
        #[arg(default_value = "")]
        prefix: String,
    },
}
