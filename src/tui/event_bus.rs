//! Event handling for TUI
//!
//! Handles keyboard input, resize events, and other terminal events.

use anyhow::Result;
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

/// Application events
#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal was resized
    Resize(u16, u16),
    /// Key was pressed
    Key(KeyEvent),
    /// Mouse event (click, scroll, etc.)
    Mouse(MouseEvent),
    /// Tick event for periodic updates
    Tick,
    /// Quit event
    Quit,
    /// Paste event (bracketed paste)
    Paste(String),
}

/// Event handler that runs in a separate thread
pub struct EventHandler {
    /// Receiver for events
    rx: mpsc::Receiver<Event>,
    /// Handle to the event thread
    handle: Option<thread::JoinHandle<()>>,
    /// Flag to signal the thread to stop
    running: Arc<AtomicBool>,
}

impl EventHandler {
    /// Create a new event handler with the given tick rate.
    ///
    /// The tick rate determines how often Tick events are generated.
    pub fn new(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = running.clone();

        let handle = thread::spawn(move || poll_events(tx, thread_running, tick_rate));

        Self {
            rx,
            handle: Some(handle),
            running,
        }
    }

    /// Get the next event, blocking until one is available.
    pub fn next(&self) -> Result<Event> {
        self.rx
            .recv()
            .map_err(|e| anyhow::anyhow!("Event channel closed: {}", e))
    }

    /// Stop the event handler thread and wait for it to exit.
    ///
    /// This must be called before spawning subprocesses that read from stdin,
    /// otherwise the event thread will race the subprocess for input.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // Thread will exit within one tick_rate cycle (250ms)
            let _ = handle.join();
        }
    }
}

/// Polls for terminal events in a loop until the running flag is cleared.
///
/// Runs inside the event thread spawned by `EventHandler::new()`. Sends events
/// through `tx` and returns when the running flag is set to false or a channel
/// error occurs.
fn poll_events(tx: mpsc::Sender<Event>, running: Arc<AtomicBool>, tick_rate: Duration) {
    while running.load(Ordering::Relaxed) {
        match event::poll(tick_rate) {
            Ok(true) => {
                // Check stop flag before blocking read
                if !running.load(Ordering::Relaxed) {
                    break;
                }
                if !dispatch_crossterm_event(&tx) {
                    break;
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
}

/// Reads one crossterm event and sends the mapped `Event` through `tx`.
///
/// Returns `false` if the loop should break (Ctrl+C pressed or channel closed),
/// `true` to continue polling.
fn dispatch_crossterm_event(tx: &mpsc::Sender<Event>) -> bool {
    match event::read() {
        Ok(CrosstermEvent::Key(key)) => {
            // Only Ctrl+C quits at the event-bus level.
            // q and Esc are forwarded as normal keys so
            // apps can handle them per-mode (e.g. cancel
            // rename, close dialog, or quit in Normal mode).
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                let _ = tx.send(Event::Quit);
                return false;
            }
            tx.send(Event::Key(key)).is_ok()
        }
        Ok(CrosstermEvent::Resize(width, height)) => tx.send(Event::Resize(width, height)).is_ok(),
        Ok(CrosstermEvent::Mouse(mouse)) => tx.send(Event::Mouse(mouse)).is_ok(),
        Ok(CrosstermEvent::Paste(text)) => tx.send(Event::Paste(text)).is_ok(),
        Ok(_) => true,
        Err(_) => false,
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

    #[test]
    fn event_handler_stop() {
        let mut handler = EventHandler::new(Duration::from_millis(50));
        handler.stop();
        // After stop, the thread should have exited and next() should fail
        assert!(handler.next().is_err());
    }

    #[test]
    fn event_paste_variant_debug() {
        let event = Event::Paste("foo".into());
        let debug = format!("{:?}", event);
        assert!(debug.contains("Paste"));
        assert!(debug.contains("foo"));
    }

    #[test]
    fn event_paste_clone() {
        let event = Event::Paste("test content".into());
        let cloned = event.clone();
        if let Event::Paste(text) = cloned {
            assert_eq!(text, "test content");
        } else {
            panic!("Expected Paste variant");
        }
    }
}
