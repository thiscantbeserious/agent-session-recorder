//! Input handling for the native player.
//!
//! This module handles keyboard and mouse input events, dispatching
//! them to the appropriate handlers and returning control flow signals.

mod keyboard;
mod mouse;

pub use keyboard::handle_key_event;
pub use mouse::handle_mouse_event;

use crate::player::state::InputResult;
use crossterm::event::Event;

use crate::asciicast::AsciicastFile;
use crate::player::state::{MarkerPosition, PlaybackState};
use crate::terminal::TerminalBuffer;

/// Handle any input event, dispatching to the appropriate handler.
///
/// # Arguments
/// * `event` - The crossterm event to handle
/// * `state` - Mutable reference to playback state
/// * `buffer` - Mutable reference to terminal buffer (for seeking)
/// * `cast` - Reference to the cast file
/// * `markers` - Reference to collected markers
/// * `total_duration` - Total duration of the recording
/// * `rec_cols` - Recording width
/// * `rec_rows` - Recording height
///
/// # Returns
/// `InputResult` indicating whether to continue, quit, or quit with file
#[allow(clippy::too_many_arguments)]
pub fn handle_event(
    event: Event,
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    markers: &[MarkerPosition],
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
) -> InputResult {
    match event {
        Event::Key(key) => handle_key_event(
            key,
            state,
            buffer,
            cast,
            markers,
            total_duration,
            rec_cols,
            rec_rows,
        ),
        Event::Mouse(mouse) => handle_mouse_event(
            mouse,
            state,
            buffer,
            cast,
            total_duration,
            rec_cols,
            rec_rows,
        ),
        Event::Resize(new_cols, new_rows) => {
            state.handle_resize(new_cols, new_rows, buffer.width(), buffer.height());
            InputResult::Continue
        }
        _ => InputResult::Continue, // Ignore focus events, etc.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asciicast::{AsciicastFile, Event as CastEvent, Header, TermInfo};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

    fn create_test_state() -> PlaybackState {
        PlaybackState::new(80, 27)
    }

    fn create_test_cast() -> AsciicastFile {
        let mut cast = AsciicastFile::new(Header {
            version: 3,
            width: Some(80),
            height: Some(24),
            term: Some(TermInfo {
                cols: Some(80),
                rows: Some(24),
                term_type: None,
            }),
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            env: None,
            idle_time_limit: None,
        });
        cast.events.push(CastEvent::output(0.1, "hello"));
        cast
    }

    #[test]
    fn handle_event_dispatches_key_event() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let key_event = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        let result = handle_event(
            key_event,
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Quit);
    }

    #[test]
    fn handle_event_dispatches_mouse_event() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        // Mouse click not on progress bar (row 0)
        let mouse_event = Event::Mouse(crossterm::event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 40,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        let result = handle_event(
            mouse_event,
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
    }

    #[test]
    fn handle_event_dispatches_resize_event() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let resize_event = Event::Resize(100, 40);
        let result = handle_event(
            resize_event,
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert_eq!(state.term_cols, 100);
        assert_eq!(state.term_rows, 40);
        assert!(state.needs_render);
    }

    #[test]
    fn handle_event_ignores_focus_events() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let focus_event = Event::FocusGained;
        let result = handle_event(
            focus_event,
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
    }

    #[test]
    fn handle_event_ignores_focus_lost() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let focus_event = Event::FocusLost;
        let result = handle_event(
            focus_event,
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
    }
}
