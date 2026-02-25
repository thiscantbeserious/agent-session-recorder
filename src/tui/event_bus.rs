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
        self.running.store(false, Ordering::Release);
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
    while running.load(Ordering::Acquire) {
        match event::poll(tick_rate) {
            Ok(true) => {
                // Check stop flag before blocking read
                if !running.load(Ordering::Acquire) {
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

/// Maps a crossterm event to an application `Event` and sends it through `tx`.
///
/// Returns `false` if the loop should break (Ctrl+C pressed or channel closed),
/// `true` to continue polling.
fn map_crossterm_event(ev: CrosstermEvent, tx: &mpsc::Sender<Event>) -> bool {
    match ev {
        CrosstermEvent::Key(key) => {
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
        CrosstermEvent::Mouse(mouse) => tx.send(Event::Mouse(mouse)).is_ok(),
        CrosstermEvent::Resize(w, h) => tx.send(Event::Resize(w, h)).is_ok(),
        CrosstermEvent::Paste(text) => tx.send(Event::Paste(text)).is_ok(),
        _ => true,
    }
}

/// Reads one crossterm event and dispatches it via `map_crossterm_event`.
///
/// Returns `false` if the loop should break (Ctrl+C pressed or channel closed),
/// `true` to continue polling.
fn dispatch_crossterm_event(tx: &mpsc::Sender<Event>) -> bool {
    match event::read() {
        Ok(ev) => map_crossterm_event(ev, tx),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn event_handler_stop_is_idempotent() {
        let mut handler = EventHandler::new(Duration::from_millis(50));
        handler.stop();
        // Calling stop() a second time should not panic
        handler.stop();
    }

    #[test]
    fn map_crossterm_event_ctrl_c_sends_quit_and_returns_false() {
        let (tx, rx) = mpsc::channel();
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let result = map_crossterm_event(CrosstermEvent::Key(key), &tx);
        assert!(
            !result,
            "Ctrl+C should return false to break the event loop"
        );
        assert!(matches!(rx.recv().unwrap(), Event::Quit));
    }

    #[test]
    fn map_crossterm_event_regular_key_sends_key_and_continues() {
        let (tx, rx) = mpsc::channel();
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let result = map_crossterm_event(CrosstermEvent::Key(key), &tx);
        assert!(result, "Regular key should return true to continue polling");
        assert!(matches!(rx.recv().unwrap(), Event::Key(_)));
    }

    #[test]
    fn map_crossterm_event_resize_sends_resize_and_continues() {
        let (tx, rx) = mpsc::channel();
        let result = map_crossterm_event(CrosstermEvent::Resize(120, 40), &tx);
        assert!(result, "Resize should return true to continue polling");
        assert!(matches!(rx.recv().unwrap(), Event::Resize(120, 40)));
    }

    #[test]
    fn map_crossterm_event_mouse_sends_mouse_and_continues() {
        let (tx, rx) = mpsc::channel();
        let mouse = crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };
        let result = map_crossterm_event(CrosstermEvent::Mouse(mouse), &tx);
        assert!(result, "Mouse event should return true to continue polling");
        assert!(matches!(rx.recv().unwrap(), Event::Mouse(_)));
    }

    #[test]
    fn map_crossterm_event_paste_sends_paste_and_continues() {
        let (tx, rx) = mpsc::channel();
        let result = map_crossterm_event(CrosstermEvent::Paste("hello".to_string()), &tx);
        assert!(result, "Paste should return true to continue polling");
        assert!(matches!(rx.recv().unwrap(), Event::Paste(ref s) if s == "hello"));
    }

    #[test]
    fn map_crossterm_event_returns_false_when_channel_closed() {
        let (tx, rx) = mpsc::channel::<Event>();
        drop(rx); // Close the receiving end
        let key = crossterm::event::KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let result = map_crossterm_event(CrosstermEvent::Key(key), &tx);
        assert!(!result, "Should return false when channel is closed");
    }
}
