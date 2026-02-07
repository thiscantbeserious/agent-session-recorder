//! Application state and event loop for TUI
//!
//! Manages the main TUI application lifecycle.

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::ResetColor,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::event::{Event, EventHandler};

/// TUI Application wrapper
///
/// Manages terminal setup/teardown and provides the main event loop.
pub struct App {
    /// Terminal instance
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Event handler
    events: EventHandler,
    /// Tick rate for event handler (needed for recreation after suspend)
    tick_rate: Duration,
    /// Whether the app should quit
    should_quit: bool,
}

impl App {
    /// Create a new App instance.
    ///
    /// Sets up the terminal for TUI mode (raw mode, alternate screen).
    /// Uses explicit rollback to ensure terminal is restored on error.
    pub fn new(tick_rate: Duration) -> Result<Self> {
        // Setup terminal with explicit rollback on error
        enable_raw_mode()?;

        // If alternate screen fails, restore raw mode
        let mut stdout = io::stdout();
        if let Err(e) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            let _ = disable_raw_mode();
            return Err(e.into());
        }

        // If terminal creation fails, restore alternate screen and raw mode
        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => {
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
                let _ = disable_raw_mode();
                return Err(e.into());
            }
        };

        let events = EventHandler::new(tick_rate);

        Ok(Self {
            terminal,
            events,
            tick_rate,
            should_quit: false,
        })
    }

    /// Get terminal size (width, height).
    pub fn size(&self) -> Result<(u16, u16)> {
        let size = self.terminal.size()?;
        Ok((size.width, size.height))
    }

    /// Get the next event from the event handler.
    pub fn next_event(&self) -> Result<Event> {
        self.events.next()
    }

    /// Check if the app should quit.
    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    /// Signal that the app should quit.
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Draw a frame using the provided closure.
    pub fn draw<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut ratatui::Frame),
    {
        self.terminal.draw(f)?;
        Ok(())
    }

    /// Suspend the TUI and restore normal terminal mode.
    ///
    /// Use this before running external commands that need the terminal.
    /// Call `resume()` afterward to re-enter TUI mode.
    ///
    /// Performs thorough terminal reset to ensure external commands
    /// (like asciinema playback) start with a clean terminal state:
    /// - Resets scroll region to full screen
    /// - Clears screen and moves cursor to home
    /// - Resets colors and attributes
    pub fn suspend(&mut self) -> Result<()> {
        // Stop the event handler thread FIRST so it releases stdin.
        // Without this, the thread races subprocesses for keyboard input.
        self.events.stop();

        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;

        // Reset terminal state for clean handoff to external commands
        // This is critical for asciinema playback which uses cursor positioning
        execute!(
            self.terminal.backend_mut(),
            ResetColor,
            crossterm::terminal::SetTitle(""), // Clear any title we may have set
            MoveTo(0, 0),
            Clear(ClearType::All)
        )?;

        // Reset scroll region to full screen (CSI r - DECSTBM reset)
        // Without this, scroll regions from the TUI can corrupt playback
        print!("\x1b[r");
        // Flush to ensure all sequences are sent before external command runs
        use std::io::Write;
        std::io::stdout().flush()?;

        Ok(())
    }

    /// Resume the TUI after a suspend.
    ///
    /// Re-enters alternate screen and raw mode, and recreates the event handler.
    /// Performs thorough terminal reset to prevent display corruption from
    /// external command output (e.g., asciinema playback).
    pub fn resume(&mut self) -> Result<()> {
        // Clear main screen residue before entering alternate screen
        // This prevents old playback output from showing through
        execute!(
            self.terminal.backend_mut(),
            ResetColor,
            MoveTo(0, 0),
            Clear(ClearType::All)
        )?;

        enable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;

        // Reset colors and clear alternate screen for clean state
        execute!(
            self.terminal.backend_mut(),
            ResetColor,
            Clear(ClearType::All)
        )?;

        self.terminal.hide_cursor()?;
        self.terminal.clear()?;

        // Recreate event handler (old one may be in bad state after suspend)
        self.events = EventHandler::new(self.tick_rate);

        Ok(())
    }
}

impl Drop for App {
    fn drop(&mut self) {
        // Restore terminal
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    // Note: App tests require a real terminal and cannot run in CI
    // The tests below verify basic type properties only

    use super::*;

    #[test]
    fn tick_rate_duration_works() {
        let tick_rate = Duration::from_millis(250);
        assert_eq!(tick_rate.as_millis(), 250);
    }
}
