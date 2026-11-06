//! Mouse input handling for the native player.
//!
//! Handles mouse events, primarily for click-to-seek on the progress bar.

use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use std::time::Instant;

use crate::asciicast::AsciicastFile;
use crate::player::playback::{find_event_index_at_time, seek_to_time};
use crate::player::state::{InputResult, PlaybackState};
use crate::terminal::TerminalBuffer;

/// Handle a mouse event.
///
/// Currently handles:
/// - Left click on progress bar to seek to that position
#[allow(clippy::too_many_arguments)]
pub fn handle_mouse_event(
    mouse: MouseEvent,
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
) -> InputResult {
    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
        let progress_row = state.term_rows.saturating_sub(2);

        if mouse.row == progress_row {
            // Calculate time from x position on progress bar
            // Bar starts at column 1, width is term_cols - 14
            let bar_start = 1u16;
            let bar_width = (state.term_cols as usize).saturating_sub(14);

            if mouse.column >= bar_start && mouse.column < bar_start + bar_width as u16 {
                let click_pos = (mouse.column - bar_start) as f64;
                let ratio = click_pos / bar_width as f64;
                let new_time = (ratio * total_duration).clamp(0.0, total_duration);

                // Exit free mode if active
                state.free_mode = false;

                // Seek to clicked position
                seek_to_time(buffer, cast, new_time, rec_cols, rec_rows);
                state.set_current_time(new_time, total_duration);
                state.set_time_offset(state.current_time());
                state.start_time = Instant::now();
                let (idx, cumulative) = find_event_index_at_time(cast, state.current_time());
                state.set_event_position(idx, cumulative, cast.events.len());

                // Resume playback after seeking
                state.paused = false;
                state.needs_render = true;
            }
        }
    }

    InputResult::Continue
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asciicast::{AsciicastFile, Event as CastEvent, Header, TermInfo};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

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
                ..Default::default()
            }),
            timestamp: None,
            duration: None,
            title: None,
            command: None,
            env: None,
            idle_time_limit: None,
            ..Default::default()
        });
        cast.events.push(CastEvent::output(0.1, "hello"));
        cast.events.push(CastEvent::output(0.2, " world"));
        cast
    }

    fn create_mouse_click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn test_progress_bar_position_calculation() {
        // Test the progress bar position calculation logic
        let term_cols: u16 = 80;
        let bar_start: u16 = 1;
        let bar_width = (term_cols as usize).saturating_sub(14); // 66

        // Click at start of bar
        let click_pos = (bar_start - bar_start) as f64;
        let ratio = click_pos / bar_width as f64;
        assert_eq!(ratio, 0.0);

        // Click at end of bar
        let click_at_end = bar_start + bar_width as u16 - 1;
        let click_pos = (click_at_end - bar_start) as f64;
        let ratio = click_pos / bar_width as f64;
        assert!((ratio - 0.984).abs() < 0.01); // ~98.4% through the bar
    }

    #[test]
    fn mouse_click_on_progress_bar_seeks() {
        let mut state = create_test_state();
        // Progress bar is at term_rows - 2
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        // Click in the middle of the progress bar (column 34 is roughly middle)
        let mouse = create_mouse_click(34, progress_row);
        let result = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        // The time should have changed based on click position
        assert!(state.current_time() > 0.0);
        assert!(state.current_time() < total_duration);
        assert!(!state.paused); // Resumes playback after seeking
    }

    #[test]
    fn mouse_click_outside_progress_bar_row_does_nothing() {
        let mut state = create_test_state();
        state.set_current_time(50.0, 100.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        // Click on a non-progress bar row
        let mouse = create_mouse_click(40, 5);
        let result = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert_eq!(state.current_time(), 50.0); // Unchanged
    }

    #[test]
    fn mouse_click_before_bar_start_does_nothing() {
        let mut state = create_test_state();
        state.set_current_time(50.0, 100.0);
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        // Click at column 0 (before bar_start of 1)
        let mouse = create_mouse_click(0, progress_row);
        let result = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert_eq!(state.current_time(), 50.0); // Unchanged
    }

    #[test]
    fn mouse_click_after_bar_end_does_nothing() {
        let mut state = create_test_state();
        state.set_current_time(50.0, 100.0);
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        // Click past the end of the bar (column 79, bar ends around column 67)
        let mouse = create_mouse_click(79, progress_row);
        let result = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert_eq!(state.current_time(), 50.0); // Unchanged
    }

    #[test]
    fn mouse_right_click_does_nothing() {
        let mut state = create_test_state();
        state.set_current_time(50.0, 100.0);
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        let mouse = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 34,
            row: progress_row,
            modifiers: KeyModifiers::NONE,
        };
        let result = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert_eq!(state.current_time(), 50.0); // Unchanged
    }

    #[test]
    fn mouse_scroll_event_does_nothing() {
        let mut state = create_test_state();
        state.set_current_time(50.0, 100.0);
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        let mouse = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 34,
            row: progress_row,
            modifiers: KeyModifiers::NONE,
        };
        let result = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert_eq!(state.current_time(), 50.0); // Unchanged
    }

    #[test]
    fn mouse_click_exits_free_mode() {
        let mut state = create_test_state();
        state.free_mode = true;
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        let mouse = create_mouse_click(34, progress_row);
        let _ = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert!(!state.free_mode);
    }

    #[test]
    fn mouse_click_at_start_of_bar_seeks_near_zero() {
        let mut state = create_test_state();
        state.set_current_time(50.0, 100.0);
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        // Click at bar_start (column 1)
        let mouse = create_mouse_click(1, progress_row);
        let _ = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        // Should be close to 0
        assert!(state.current_time() < 5.0);
    }

    #[test]
    fn mouse_click_near_end_of_bar_seeks_near_duration() {
        let mut state = create_test_state();
        state.set_current_time(0.0, 100.0);
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;

        // Bar width is term_cols - 14 = 66, bar_start is 1
        // So bar ends at 1 + 66 - 1 = 66
        let mouse = create_mouse_click(65, progress_row);
        let _ = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        // Should be close to end
        assert!(state.current_time() > 90.0);
    }

    #[test]
    fn mouse_click_updates_timing_state() {
        let mut state = create_test_state();
        let progress_row = state.term_rows - 2;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 100.0;
        let old_start_time = state.start_time;

        // Small delay to ensure different start_time
        std::thread::sleep(std::time::Duration::from_millis(1));

        let mouse = create_mouse_click(34, progress_row);
        let _ = handle_mouse_event(
            mouse,
            &mut state,
            &mut buffer,
            &cast,
            total_duration,
            80,
            24,
        );

        assert!(state.start_time > old_start_time);
        assert_eq!(state.time_offset(), state.current_time());
    }
}
