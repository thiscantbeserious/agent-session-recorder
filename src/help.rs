//! Top-level help, logo and version output for the AGR binary.
//!
//! Handles the interactive TUI help screen and themed clap help rendering.

use anyhow::Result;
use clap::CommandFactory;
use terminal_size::{terminal_size, Width};

use agr::cli::{build_cli_styles, Cli};
use agr::theme;
use agr::tui;

/// Generate the ASCII logo with dynamic-width REC line.
///
/// Uses the TUI module's static logo builder for consistency.
pub fn build_logo() -> String {
    let width = terminal_size()
        .map(|(Width(w), _)| w as usize)
        .unwrap_or(80);

    tui::widgets::logo::build_static_logo(width)
}

/// Build version string.
///
/// For dev builds (default): "0.1.0-dev+abc1234 (owner/repo, built 2025-01-21)"
/// For release builds (--features release): "0.1.0 (owner/repo, built 2025-01-21)"
pub fn build_version() -> &'static str {
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
pub fn should_show_tui_help() -> bool {
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
pub fn show_tui_help() -> Result<()> {
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
pub fn print_themed_help(err: &clap::Error) {
    // Get the rendered help from the error
    let help_text = err.to_string();
    let colored = theme::colorize_help(&help_text);
    print!("{}", colored);
}

/// Parse the command line, printing themed help and exiting for help and usage errors.
pub fn parse_matches(cmd: clap::Command) -> clap::ArgMatches {
    match cmd.try_get_matches() {
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
    }
}
