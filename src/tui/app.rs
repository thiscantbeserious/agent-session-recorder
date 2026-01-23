//! Application state and event loop for TUI
//!
//! Manages the main TUI application lifecycle.

use std::io::{self, Stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
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
