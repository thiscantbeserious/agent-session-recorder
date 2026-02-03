//! Agent Session Recorder (AGR) - CLI entry point
//!
//! This file handles argument parsing and command dispatch.
//! CLI struct definitions are in the `cli` module (src/cli.rs).
//! Command implementations are in the `commands` module.

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};
use terminal_size::{terminal_size, Width};

mod commands;

use agr::cli::{
    build_cli_styles, AgentCommands, Cli, Commands, ConfigCommands, MarkerCommands, NoWrapCommands,
    ShellCommands,
};
use agr::tui;

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

    // Build command with styles, logo, and custom version
    let cmd = Cli::command()
        .styles(build_cli_styles())
        .before_help(build_logo())
        .version(build_version());

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
        Commands::Play { file } => commands::play::handle(&file),
        Commands::Copy { file } => commands::copy::handle(&file),
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
            ConfigCommands::Migrate { yes } => commands::config::handle_migrate(yes),
        },
        Commands::Shell(cmd) => match cmd {
            ShellCommands::Status => commands::shell::handle_status(),
            ShellCommands::Install => commands::shell::handle_install(),
            ShellCommands::Uninstall => commands::shell::handle_uninstall(),
        },
        Commands::Optimize {
            remove_silence,
            output,
            file,
        } => {
            // Parse the threshold from the optional string value
            let threshold = match remove_silence {
                Some(ref s) if !s.is_empty() => {
                    let parsed: f64 = s.parse().map_err(|_| {
                        anyhow::anyhow!("Invalid threshold '{}': must be a positive number", s)
                    })?;
                    Some(parsed)
                }
                _ => None, // No value provided, will use header or default
            };

            // Currently only silence removal is supported
            if remove_silence.is_none() {
                anyhow::bail!("No optimization specified. Use --remove-silence to remove silence.");
            }

            commands::transform::handle_remove_silence(&file, threshold, output.as_deref())
        }
        Commands::Completions {
            shell,
            shell_init,
            debug,
            files,
            limit,
            prefix,
        } => commands::completions::handle::<Cli>(shell, shell_init, debug, files, limit, &prefix),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};
    use clap_complete::Shell as CompletionShell;

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
                ..
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
                ..
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
                ..
            } => {
                assert!(shell.is_none());
                assert!(files);
                assert_eq!(prefix, "claude/");
            }
            _ => panic!("Expected Completions command"),
        }
    }

    #[test]
    fn cli_completions_parses_with_shell_init_zsh() {
        let cli = Cli::try_parse_from(["agr", "completions", "--shell-init", "zsh"]).unwrap();
        match cli.command {
            Commands::Completions {
                shell,
                shell_init,
                debug,
                files,
                limit,
                prefix,
            } => {
                assert!(shell.is_none());
                assert_eq!(shell_init, Some(CompletionShell::Zsh));
                assert!(!debug);
                assert!(!files);
                assert_eq!(limit, 10);
                assert_eq!(prefix, "");
            }
            _ => panic!("Expected Completions command"),
        }
    }

    #[test]
    fn cli_completions_parses_with_shell_init_bash() {
        let cli = Cli::try_parse_from(["agr", "completions", "--shell-init", "bash"]).unwrap();
        match cli.command {
            Commands::Completions { shell_init, .. } => {
                assert_eq!(shell_init, Some(CompletionShell::Bash));
            }
            _ => panic!("Expected Completions command"),
        }
    }

    #[test]
    fn cli_completions_parses_with_limit() {
        let cli = Cli::try_parse_from(["agr", "completions", "--files", "--limit", "20"]).unwrap();
        match cli.command {
            Commands::Completions { files, limit, .. } => {
                assert!(files);
                assert_eq!(limit, 20);
            }
            _ => panic!("Expected Completions command"),
        }
    }

    #[test]
    fn cli_completions_limit_defaults_to_10() {
        let cli = Cli::try_parse_from(["agr", "completions", "--files"]).unwrap();
        match cli.command {
            Commands::Completions { limit, .. } => {
                assert_eq!(limit, 10);
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
    fn cli_config_migrate_parses() {
        let cli = Cli::try_parse_from(["agr", "config", "migrate"]).unwrap();
        match cli.command {
            Commands::Config(ConfigCommands::Migrate { yes }) => {
                assert!(!yes);
            }
            _ => panic!("Expected Config Migrate command"),
        }
    }

    #[test]
    fn cli_config_migrate_parses_with_yes_flag() {
        let cli = Cli::try_parse_from(["agr", "config", "migrate", "--yes"]).unwrap();
        match cli.command {
            Commands::Config(ConfigCommands::Migrate { yes }) => {
                assert!(yes);
            }
            _ => panic!("Expected Config Migrate command"),
        }
    }

    #[test]
    fn cli_config_migrate_parses_with_short_yes_flag() {
        let cli = Cli::try_parse_from(["agr", "config", "migrate", "-y"]).unwrap();
        match cli.command {
            Commands::Config(ConfigCommands::Migrate { yes }) => {
                assert!(yes);
            }
            _ => panic!("Expected Config Migrate command"),
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

    #[test]
    fn cli_play_parses_with_file() {
        let cli = Cli::try_parse_from(["agr", "play", "session.cast"]).unwrap();
        match cli.command {
            Commands::Play { file } => {
                assert_eq!(file, "session.cast");
            }
            _ => panic!("Expected Play command"),
        }
    }

    #[test]
    fn cli_play_parses_with_path() {
        let cli = Cli::try_parse_from(["agr", "play", "/path/to/session.cast"]).unwrap();
        match cli.command {
            Commands::Play { file } => {
                assert_eq!(file, "/path/to/session.cast");
            }
            _ => panic!("Expected Play command"),
        }
    }

    #[test]
    fn cli_play_parses_with_short_format() {
        let cli = Cli::try_parse_from(["agr", "play", "claude/session.cast"]).unwrap();
        match cli.command {
            Commands::Play { file } => {
                assert_eq!(file, "claude/session.cast");
            }
            _ => panic!("Expected Play command"),
        }
    }

    #[test]
    fn cli_copy_parses_with_file() {
        let cli = Cli::try_parse_from(["agr", "copy", "session.cast"]).unwrap();
        match cli.command {
            Commands::Copy { file } => {
                assert_eq!(file, "session.cast");
            }
            _ => panic!("Expected Copy command"),
        }
    }

    #[test]
    fn cli_copy_parses_with_path() {
        let cli = Cli::try_parse_from(["agr", "copy", "/path/to/session.cast"]).unwrap();
        match cli.command {
            Commands::Copy { file } => {
                assert_eq!(file, "/path/to/session.cast");
            }
            _ => panic!("Expected Copy command"),
        }
    }

    #[test]
    fn cli_copy_parses_with_short_format() {
        let cli = Cli::try_parse_from(["agr", "copy", "claude/session.cast"]).unwrap();
        match cli.command {
            Commands::Copy { file } => {
                assert_eq!(file, "claude/session.cast");
            }
            _ => panic!("Expected Copy command"),
        }
    }

    #[test]
    fn cli_copy_requires_file_argument() {
        // `agr copy` without file should fail
        let result = Cli::try_parse_from(["agr", "copy"]);
        assert!(result.is_err());
    }
}
