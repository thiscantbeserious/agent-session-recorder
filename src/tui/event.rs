//! Event handling for TUI
//!
//! Handles keyboard input, resize events, and other terminal events.

use anyhow::Result;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal was resized
    Resize(u16, u16),
    /// Key was pressed
    Key(KeyEvent),
    /// Tick event for periodic updates
    Tick,
    /// Quit event
    Quit,
}

/// Event handler that runs in a separate thread
pub struct EventHandler {
    /// Receiver for events
    rx: mpsc::Receiver<Event>,
    /// Handle to the event thread (kept for cleanup)
    _handle: thread::JoinHandle<()>,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate.
    ///
    /// The tick rate determines how often Tick events are generated.
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            loop {
                // Poll for events with timeout using explicit pattern matching
                match event::poll(tick_rate) {
                    Ok(true) => {
                        // Event available
                        match event::read() {
                            Ok(CrosstermEvent::Key(key)) => {
                                // Check for quit keys
                                if key.code == KeyCode::Char('q')
                                    || key.code == KeyCode::Esc
                                    || (key.code == KeyCode::Char('c')
                                        && key.modifiers.contains(KeyModifiers::CONTROL))
                                {
                                    let _ = tx.send(Event::Quit);
                                    break;
                                }
                                if tx.send(Event::Key(key)).is_err() {
                                    break;
                                }
                            }
                            Ok(CrosstermEvent::Resize(width, height)) => {
                                if tx.send(Event::Resize(width, height)).is_err() {
                                    break;
                                }
                            }
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
                    Ok(false) => {
                        // Timeout - send tick event
                        if tx.send(Event::Tick).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Polling error - exit the event loop
                        break;
                    }
                }
            }
        });

        Self {
            rx,
            _handle: handle,
        }
    }

    /// Get the next event, blocking until one is available.
    pub fn next(&self) -> Result<Event> {
        self.rx
            .recv()
            .map_err(|e| anyhow::anyhow!("Event channel closed: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_debug_format() {
        let event = Event::Resize(80, 24);
        let debug = format!("{:?}", event);
        assert!(debug.contains("Resize"));
        assert!(debug.contains("80"));
        assert!(debug.contains("24"));
    }

    #[test]
    fn event_clone_works() {
        let event = Event::Tick;
        let cloned = event.clone();
        assert!(matches!(cloned, Event::Tick));
    }
}
