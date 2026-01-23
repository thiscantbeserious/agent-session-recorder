//! Agent Session Recorder (AGR) - CLI entry point
//!
//! This file contains CLI struct definitions, argument parsing, and dispatch.
//! Command implementations are in the `commands` module.

use anyhow::Result;
use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::Shell as CompletionShell;
use terminal_size::{terminal_size, Width};

mod commands;

use agr::tui;

/// Build clap styles using our theme colors.
///
/// Maps theme colors to clap's styling system for consistent CLI appearance.
/// - Green: headers, usage, command names (accent color)
/// - White: descriptions, placeholders (renders as light gray on dark terminals)
fn build_cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Green.on_default())
        .placeholder(AnsiColor::White.on_default()) // Light gray for descriptions
        .valid(AnsiColor::White.on_default()) // Light gray for valid values
        .invalid(AnsiColor::Red.on_default())
        .error(AnsiColor::Red.on_default() | Effects::BOLD)
}

/// Generate the ASCII logo with dynamic-width REC line.
///
/// Uses the TUI module's static logo builder for consistency.
fn build_logo() -> String {
    let width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    tui::widgets::logo::build_static_logo(width)
}

/// Build version string.
///
/// For dev builds (default): "0.1.0-dev+abc1234 (owner/repo, built 2025-01-21)"
/// For release builds (--features release): "0.1.0 (owner/repo, built 2025-01-21)"
fn build_version() -> &'static str {
    #[cfg(not(feature = "release"))]
    {
        // Dev build: include git hash
        // Environment variables are set by build.rs via vergen
        const GIT_SHA: &str = env!("VERGEN_GIT_SHA");
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        const BUILD_DATE: &str = env!("AGR_BUILD_DATE");
        const REPO_NAME: &str = env!("AGR_REPO_NAME");

        // Use OnceLock for lazy initialization of the version string
        static VERSION_STRING: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        VERSION_STRING.get_or_init(|| {
            let version_part =
                if GIT_SHA.is_empty() || GIT_SHA == "unknown" || GIT_SHA.starts_with("VERGEN_") {
                    format!("{}-dev", VERSION)
                } else {
                    // Take first 7 characters of SHA for short hash
                    let short_sha = if GIT_SHA.len() > 7 {
                        &GIT_SHA[..7]
                    } else {
                        GIT_SHA
                    };
                    format!("{}-dev+{}", VERSION, short_sha)
                };
            format!("{} ({}, built {})", version_part, REPO_NAME, BUILD_DATE)
        })
    }

    #[cfg(feature = "release")]
    {
        // Release build: clean version with repo and build date
        const VERSION: &str = env!("CARGO_PKG_VERSION");
        const BUILD_DATE: &str = env!("AGR_BUILD_DATE");
        const REPO_NAME: &str = env!("AGR_REPO_NAME");

        static VERSION_STRING: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        VERSION_STRING.get_or_init(|| format!("{} ({}, built {})", VERSION, REPO_NAME, BUILD_DATE))
    }
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
#[command(version = build_version())]
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

/// Check if we should show TUI help.
///
/// Returns true if:
/// - The user passed --help or -h as the only argument (or with agr)
/// - Output is a TTY (not piped)
fn should_show_tui_help() -> bool {
    let args: Vec<String> = std::env::args().collect();

    // Check if --help or -h is present as the only argument after program name
    // We want TUI help only for top-level help, not subcommand help
    let is_help_request =
        args.len() == 2 && (args[1] == "--help" || args[1] == "-h" || args[1] == "help");

    // Check if stdout is a TTY
    let is_tty = atty::is(atty::Stream::Stdout);

    is_help_request && is_tty
}

use ratatui::{backend::CrosstermBackend, Terminal};

/// RAII guard for terminal cleanup.
///
/// Ensures terminal is restored to normal state even if an error occurs.
struct TerminalGuard<W: std::io::Write> {
    terminal: Terminal<CrosstermBackend<W>>,
}

impl<W: std::io::Write> TerminalGuard<W> {
    fn new(terminal: Terminal<CrosstermBackend<W>>) -> Self {
        Self { terminal }
    }
}

impl<W: std::io::Write> Drop for TerminalGuard<W> {
    fn drop(&mut self) {
        use crossterm::{
            execute,
            terminal::{disable_raw_mode, LeaveAlternateScreen},
        };
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

impl<W: std::io::Write> std::ops::Deref for TerminalGuard<W> {
    type Target = Terminal<CrosstermBackend<W>>;

    fn deref(&self) -> &Self::Target {
        &self.terminal
    }
}

impl<W: std::io::Write> std::ops::DerefMut for TerminalGuard<W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.terminal
    }
}

/// Show interactive TUI help screen.
///
/// Displays the logo with dynamic REC line that responds to terminal resize,
/// plus scrollable help content below.
fn show_tui_help() -> Result<()> {
    use crossterm::{
        event::{self, Event as CrosstermEvent, KeyCode, KeyModifiers},
        execute,
        terminal::{enable_raw_mode, EnterAlternateScreen},
    };
    use std::io;
    use std::time::Duration;

    // Generate help text (without the logo - we render that separately)
    let help_text = {
        let mut cmd = Cli::command().styles(build_cli_styles());
        let mut buf = Vec::new();
        cmd.write_long_help(&mut buf)?;
        String::from_utf8_lossy(&buf).to_string()
    };

    // Count total lines for scroll bounds
    let total_lines = help_text.lines().count() as u16;

    // Setup terminal with RAII guard for cleanup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    let mut terminal = TerminalGuard::new(terminal);

    let mut scroll_offset: u16 = 0;

    // Draw loop
    loop {
        let visible_height = terminal
            .size()?
            .height
            .saturating_sub(tui::widgets::Logo::height() + 1);
        let max_scroll = total_lines.saturating_sub(visible_height);

        // Draw
        terminal.draw(|frame| {
            tui::ui::render_help(frame, &help_text, scroll_offset);
        })?;

        // Handle events
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                CrosstermEvent::Key(key) => {
                    match key.code {
                        // Exit
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break
                        }
                        // Scroll down
                        KeyCode::Down | KeyCode::Char('j') => {
                            scroll_offset = scroll_offset.saturating_add(1).min(max_scroll);
                        }
                        // Scroll up
                        KeyCode::Up | KeyCode::Char('k') => {
                            scroll_offset = scroll_offset.saturating_sub(1);
                        }
                        // Page down
                        KeyCode::PageDown | KeyCode::Char(' ') => {
                            scroll_offset =
                                scroll_offset.saturating_add(visible_height).min(max_scroll);
                        }
                        // Page up
                        KeyCode::PageUp => {
                            scroll_offset = scroll_offset.saturating_sub(visible_height);
                        }
                        // Home
                        KeyCode::Home => {
                            scroll_offset = 0;
                        }
                        // End
                        KeyCode::End => {
                            scroll_offset = max_scroll;
                        }
                        _ => {}
                    }
                }
                CrosstermEvent::Resize(_, _) => {
                    // Terminal resized - just redraw (logo will adapt)
                }
                _ => {}
            }
        }
    }

    // Terminal is restored automatically by TerminalGuard's Drop impl
    Ok(())
}

/// Print help with theme-based colorization.
///
/// Takes clap's error which contains the rendered help, applies theme colors,
/// and prints the result.
fn print_themed_help(err: &clap::Error) {
    // Get the rendered help from the error
    let help_text = err.to_string();
    let colored = tui::colorize_help(&help_text);
    print!("{}", colored);
}

#[cfg(not(tarpaulin_include))]
fn main() -> Result<()> {
    // Check for interactive TUI help
    if should_show_tui_help() {
        return show_tui_help();
    }

    // Build command with styles and logo
    let cmd = Cli::command()
        .styles(build_cli_styles())
        .before_help(build_logo());

    // Try to parse, handling help requests with themed output
    let matches = match cmd.try_get_matches() {
        Ok(m) => m,
        Err(e) => {
            match e.kind() {
                clap::error::ErrorKind::DisplayHelp => {
                    // For help requests, use themed colorization
                    print_themed_help(&e);
                    std::process::exit(0);
                }
                clap::error::ErrorKind::DisplayVersion => {
                    // Version is handled normally
                    e.exit();
                }
                clap::error::ErrorKind::MissingSubcommand
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    // Missing subcommand/argument shows help - colorize it
                    print_themed_help(&e);
                    std::process::exit(2);
                }
                _ => {
                    // Other errors (invalid args, etc.)
                    e.exit();
                }
            }
        }
    };

    let cli = Cli::from_arg_matches(&matches).unwrap();

    match cli.command {
        Commands::Record { agent, name, args } => {
            commands::record::handle(&agent, name.as_deref(), &args)
        }
        Commands::Status => commands::status::handle(),
        Commands::Cleanup { agent, older_than } => {
            commands::cleanup::handle(agent.as_deref(), older_than)
        }
        Commands::List { agent } => commands::list::handle(agent.as_deref()),
        Commands::Analyze { file, agent } => commands::analyze::handle(&file, agent.as_deref()),
        Commands::Marker(cmd) => match cmd {
            MarkerCommands::Add { file, time, label } => {
                commands::marker::handle_add(&file, time, &label)
            }
            MarkerCommands::List { file } => commands::marker::handle_list(&file),
        },
        Commands::Agents(cmd) => match cmd {
            AgentCommands::List => commands::agents::handle_list(),
            AgentCommands::Add { name } => commands::agents::handle_add(&name),
            AgentCommands::Remove { name } => commands::agents::handle_remove(&name),
            AgentCommands::IsWrapped { name } => commands::agents::handle_is_wrapped(&name),
            AgentCommands::NoWrap(nowrap_cmd) => match nowrap_cmd {
                NoWrapCommands::List => commands::agents::handle_nowrap_list(),
                NoWrapCommands::Add { name } => commands::agents::handle_nowrap_add(&name),
                NoWrapCommands::Remove { name } => commands::agents::handle_nowrap_remove(&name),
            },
        },
        Commands::Config(cmd) => match cmd {
            ConfigCommands::Show => commands::config::handle_show(),
            ConfigCommands::Edit => commands::config::handle_edit(),
        },
        Commands::Shell(cmd) => match cmd {
            ShellCommands::Status => commands::shell::handle_status(),
            ShellCommands::Install => commands::shell::handle_install(),
            ShellCommands::Uninstall => commands::shell::handle_uninstall(),
        },
        Commands::Completions {
            shell,
            files,
            prefix,
        } => commands::completions::handle::<Cli>(shell, files, &prefix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

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

    #[test]
    fn cli_analyze_parses_with_file_only() {
        let cli = Cli::try_parse_from(["agr", "analyze", "session.cast"]).unwrap();
        match cli.command {
            Commands::Analyze { file, agent } => {
                assert_eq!(file, "session.cast");
                assert!(agent.is_none());
            }
            _ => panic!("Expected Analyze command"),
        }
    }

    #[test]
    fn cli_analyze_parses_with_agent_flag() {
        let cli =
            Cli::try_parse_from(["agr", "analyze", "session.cast", "--agent", "codex"]).unwrap();
        match cli.command {
            Commands::Analyze { file, agent } => {
                assert_eq!(file, "session.cast");
                assert_eq!(agent, Some("codex".to_string()));
            }
            _ => panic!("Expected Analyze command"),
        }
    }

    #[test]
    fn cli_analyze_parses_with_short_agent_flag() {
        let cli = Cli::try_parse_from(["agr", "analyze", "session.cast", "-a", "claude"]).unwrap();
        match cli.command {
            Commands::Analyze { file, agent } => {
                assert_eq!(file, "session.cast");
                assert_eq!(agent, Some("claude".to_string()));
            }
            _ => panic!("Expected Analyze command"),
        }
    }

    #[test]
    fn cli_analyze_parses_with_path() {
        let cli = Cli::try_parse_from(["agr", "analyze", "/path/to/session.cast"]).unwrap();
        match cli.command {
            Commands::Analyze { file, agent } => {
                assert_eq!(file, "/path/to/session.cast");
                assert!(agent.is_none());
            }
            _ => panic!("Expected Analyze command"),
        }
    }

    #[test]
    fn cli_completions_parses_with_shell_flag() {
        let cli = Cli::try_parse_from(["agr", "completions", "--shell", "bash"]).unwrap();
        match cli.command {
            Commands::Completions {
                shell,
                files,
                prefix,
            } => {
                assert_eq!(shell, Some(CompletionShell::Bash));
                assert!(!files);
                assert_eq!(prefix, "");
            }
            _ => panic!("Expected Completions command"),
        }
    }

    #[test]
    fn cli_completions_parses_with_files_flag() {
        let cli = Cli::try_parse_from(["agr", "completions", "--files"]).unwrap();
        match cli.command {
            Commands::Completions {
                shell,
                files,
                prefix,
            } => {
                assert!(shell.is_none());
                assert!(files);
                assert_eq!(prefix, "");
            }
            _ => panic!("Expected Completions command"),
        }
    }

    #[test]
    fn cli_completions_parses_with_files_and_prefix() {
        let cli = Cli::try_parse_from(["agr", "completions", "--files", "claude/"]).unwrap();
        match cli.command {
            Commands::Completions {
                shell,
                files,
                prefix,
            } => {
                assert!(shell.is_none());
                assert!(files);
                assert_eq!(prefix, "claude/");
            }
            _ => panic!("Expected Completions command"),
        }
    }

    #[test]
    fn cli_completions_is_hidden() {
        // The completions command should not appear in --help output
        let cmd = Cli::command();
        let subcommands: Vec<_> = cmd.get_subcommands().collect();
        let completions_cmd = subcommands.iter().find(|c| c.get_name() == "completions");
        assert!(
            completions_cmd.is_some(),
            "Completions command should exist"
        );
        assert!(
            completions_cmd.unwrap().is_hide_set(),
            "Completions command should be hidden"
        );
    }

    #[test]
    fn cli_list_has_ls_alias() {
        // Test that 'ls' is accepted as an alias for 'list'
        let cli = Cli::try_parse_from(["agr", "ls"]).unwrap();
        match cli.command {
            Commands::List { agent } => {
                assert!(agent.is_none());
            }
            _ => panic!("Expected List command from 'ls' alias"),
        }
    }

    #[test]
    fn cli_ls_alias_accepts_agent_argument() {
        let cli = Cli::try_parse_from(["agr", "ls", "claude"]).unwrap();
        match cli.command {
            Commands::List { agent } => {
                assert_eq!(agent, Some("claude".to_string()));
            }
            _ => panic!("Expected List command from 'ls' alias with agent"),
        }
    }

    #[test]
    fn cli_list_alias_is_visible() {
        // The 'ls' alias should be visible in help
        let cmd = Cli::command();
        let subcommands: Vec<_> = cmd.get_subcommands().collect();
        let list_cmd = subcommands.iter().find(|c| c.get_name() == "list");
        assert!(list_cmd.is_some(), "List command should exist");

        // Check that visible_alias is set
        let aliases: Vec<_> = list_cmd.unwrap().get_visible_aliases().collect();
        assert!(
            aliases.contains(&"ls"),
            "List command should have 'ls' as visible alias"
        );
    }

    #[test]
    fn cli_marker_add_parses() {
        let cli =
            Cli::try_parse_from(["agr", "marker", "add", "test.cast", "45.2", "marker label"])
                .unwrap();
        match cli.command {
            Commands::Marker(MarkerCommands::Add { file, time, label }) => {
                assert_eq!(file, "test.cast");
                assert!((time - 45.2).abs() < f64::EPSILON);
                assert_eq!(label, "marker label");
            }
            _ => panic!("Expected Marker Add command"),
        }
    }

    #[test]
    fn cli_marker_list_parses() {
        let cli = Cli::try_parse_from(["agr", "marker", "list", "test.cast"]).unwrap();
        match cli.command {
            Commands::Marker(MarkerCommands::List { file }) => {
                assert_eq!(file, "test.cast");
            }
            _ => panic!("Expected Marker List command"),
        }
    }

    #[test]
    fn cli_record_parses_with_agent_only() {
        let cli = Cli::try_parse_from(["agr", "record", "claude"]).unwrap();
        match cli.command {
            Commands::Record { agent, name, args } => {
                assert_eq!(agent, "claude");
                assert!(name.is_none());
                assert!(args.is_empty());
            }
            _ => panic!("Expected Record command"),
        }
    }

    #[test]
    fn cli_record_parses_with_name() {
        let cli = Cli::try_parse_from(["agr", "record", "claude", "--name", "my-session"]).unwrap();
        match cli.command {
            Commands::Record { agent, name, args } => {
                assert_eq!(agent, "claude");
                assert_eq!(name, Some("my-session".to_string()));
                assert!(args.is_empty());
            }
            _ => panic!("Expected Record command"),
        }
    }

    #[test]
    fn cli_record_parses_with_passthrough_args() {
        let cli =
            Cli::try_parse_from(["agr", "record", "claude", "--", "--help", "some-arg"]).unwrap();
        match cli.command {
            Commands::Record { agent, name, args } => {
                assert_eq!(agent, "claude");
                assert!(name.is_none());
                assert_eq!(args, vec!["--help", "some-arg"]);
            }
            _ => panic!("Expected Record command"),
        }
    }

    #[test]
    fn cli_agents_list_parses() {
        let cli = Cli::try_parse_from(["agr", "agents", "list"]).unwrap();
        match cli.command {
            Commands::Agents(AgentCommands::List) => {}
            _ => panic!("Expected Agents List command"),
        }
    }

    #[test]
    fn cli_agents_add_parses() {
        let cli = Cli::try_parse_from(["agr", "agents", "add", "my-agent"]).unwrap();
        match cli.command {
            Commands::Agents(AgentCommands::Add { name }) => {
                assert_eq!(name, "my-agent");
            }
            _ => panic!("Expected Agents Add command"),
        }
    }

    #[test]
    fn cli_agents_remove_parses() {
        let cli = Cli::try_parse_from(["agr", "agents", "remove", "old-agent"]).unwrap();
        match cli.command {
            Commands::Agents(AgentCommands::Remove { name }) => {
                assert_eq!(name, "old-agent");
            }
            _ => panic!("Expected Agents Remove command"),
        }
    }

    #[test]
    fn cli_config_show_parses() {
        let cli = Cli::try_parse_from(["agr", "config", "show"]).unwrap();
        match cli.command {
            Commands::Config(ConfigCommands::Show) => {}
            _ => panic!("Expected Config Show command"),
        }
    }

    #[test]
    fn cli_config_edit_parses() {
        let cli = Cli::try_parse_from(["agr", "config", "edit"]).unwrap();
        match cli.command {
            Commands::Config(ConfigCommands::Edit) => {}
            _ => panic!("Expected Config Edit command"),
        }
    }

    #[test]
    fn cli_status_parses() {
        let cli = Cli::try_parse_from(["agr", "status"]).unwrap();
        match cli.command {
            Commands::Status => {}
            _ => panic!("Expected Status command"),
        }
    }
}
