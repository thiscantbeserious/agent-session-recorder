//! Keyboard input handling for the native player.
//!
//! Handles all keyboard shortcuts including playback controls,
//! navigation, mode toggles, and seeking.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::asciicast::AsciicastFile;
use crate::player::playback::{find_event_index_at_time, seek_to_time};
use crate::player::state::{InputResult, MarkerPosition, PlaybackState};
use crate::terminal::TerminalBuffer;

/// Handle a keyboard event.
///
/// This is the main keyboard input handler that processes all key events
/// and updates state or returns control flow signals.
#[allow(clippy::too_many_arguments)]
pub fn handle_key_event(
    key: KeyEvent,
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    markers: &[MarkerPosition],
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
) -> InputResult {
    // If help is showing, any key closes it
    if state.show_help {
        state.show_help = false;
        state.needs_render = true;
        return InputResult::Continue;
    }

    match key.code {
        // === Quit ===
        KeyCode::Char('q') => InputResult::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => InputResult::Quit,
        KeyCode::Esc => {
            if state.exit_mode_or_quit() {
                InputResult::Continue
            } else {
                InputResult::Quit
            }
        }

        // === Mode toggles ===
        KeyCode::Char('?') => {
            state.toggle_help();
            InputResult::Continue
        }
        KeyCode::Char('v') => {
            state.toggle_viewport_mode();
            InputResult::Continue
        }
        KeyCode::Char('f') => {
            state.toggle_free_mode(buffer.cursor_row());
            InputResult::Continue
        }

        // === Playback controls ===
        KeyCode::Char(' ') => {
            state.toggle_pause();
            InputResult::Continue
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            state.speed_up();
            InputResult::Continue
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            state.speed_down();
            InputResult::Continue
        }

        // === Resize terminal ===
        KeyCode::Char('r') => {
            handle_resize_to_recording(state, buffer);
            InputResult::Continue
        }

        // === Marker navigation ===
        KeyCode::Char('m') => {
            handle_jump_to_marker(state, buffer, cast, markers, rec_cols, rec_rows);
            InputResult::Continue
        }

        // === Seeking ===
        KeyCode::Char('<') | KeyCode::Char(',') => {
            handle_seek_backward(state, buffer, cast, 5.0, rec_cols, rec_rows);
            InputResult::Continue
        }
        KeyCode::Char('>') | KeyCode::Char('.') => {
            handle_seek_forward(state, buffer, cast, 5.0, total_duration, rec_cols, rec_rows);
            InputResult::Continue
        }
        KeyCode::Home => {
            handle_seek_to_start(state, buffer, cast, rec_cols, rec_rows);
            InputResult::Continue
        }
        KeyCode::End => {
            handle_seek_to_end(state, buffer, cast, total_duration, rec_cols, rec_rows);
            InputResult::Continue
        }

        // === Arrow keys (context-dependent) ===
        KeyCode::Left => {
            handle_left_key(
                state,
                buffer,
                cast,
                key.modifiers,
                total_duration,
                rec_cols,
                rec_rows,
            );
            InputResult::Continue
        }
        KeyCode::Right => {
            handle_right_key(
                state,
                buffer,
                cast,
                key.modifiers,
                total_duration,
                rec_cols,
                rec_rows,
            );
            InputResult::Continue
        }
        KeyCode::Up => {
            handle_up_key(state);
            InputResult::Continue
        }
        KeyCode::Down => {
            handle_down_key(state, buffer);
            InputResult::Continue
        }

        _ => InputResult::Continue,
    }
}

/// Handle resize terminal to match current buffer size.
fn handle_resize_to_recording(state: &mut PlaybackState, buffer: &TerminalBuffer) {
    // NOTE: This uses xterm escape sequence which only works on
    // xterm-compatible terminals (iTerm2, xterm, etc.)
    let buf_rows = buffer.height();
    let buf_cols = buffer.width();
    let target_rows = buf_rows as u32 + PlaybackState::STATUS_LINES as u32;
    let mut stdout = io::stdout();
    // Intentionally ignore errors - terminal may not support xterm resize sequences
    let _ = write!(stdout, "\x1b[8;{};{}t", target_rows, buf_cols);
    let _ = stdout.flush();

    // Small delay for terminal to resize
    std::thread::sleep(Duration::from_millis(50));

    // Update view dimensions after resize
    if let Ok((new_cols, new_rows)) = crossterm::terminal::size() {
        state.term_cols = new_cols;
        state.term_rows = new_rows;
        state.view_rows = (new_rows.saturating_sub(PlaybackState::STATUS_LINES)) as usize;
        state.view_cols = new_cols as usize;

        // Check if resize succeeded (terminal at least as big as buffer)
        let resize_ok = new_cols as usize >= buf_cols
            && new_rows as usize >= PlaybackState::STATUS_LINES as usize + buf_rows;
        if resize_ok {
            // Reset viewport offset since we now fit
            if state.view_rows >= buf_rows {
                state.set_view_row_offset(0, 0);
            }
            if state.view_cols >= buf_cols {
                state.set_view_col_offset(0, 0);
            }
        } else {
            // Clamp to new maximums in case the viewport grew but still doesn't fit
            let max_row_offset = buf_rows.saturating_sub(state.view_rows);
            let max_col_offset = buf_cols.saturating_sub(state.view_cols);
            state.set_view_row_offset(state.view_row_offset(), max_row_offset);
            state.set_view_col_offset(state.view_col_offset(), max_col_offset);
        }
    }
    state.needs_render = true;
}

/// Handle jump to next marker.
fn handle_jump_to_marker(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    markers: &[MarkerPosition],
    rec_cols: u32,
    rec_rows: u32,
) {
    if let Some(next) = markers.iter().find(|m| m.time > state.current_time() + 0.1) {
        seek_to_time(buffer, cast, next.time, rec_cols, rec_rows);
        state.set_current_time(next.time, f64::MAX);
        state.set_time_offset(state.current_time());
        state.start_time = Instant::now();
        let (idx, cumulative) = find_event_index_at_time(cast, state.current_time());
        state.set_event_position(idx, cumulative, cast.events.len());
        state.paused = true;
        state.needs_render = true;
    }
}

/// Handle seeking backward by a given amount.
fn handle_seek_backward(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    amount: f64,
    rec_cols: u32,
    rec_rows: u32,
) {
    let new_time = (state.current_time() - amount).max(0.0);
    seek_to_time(buffer, cast, new_time, rec_cols, rec_rows);
    state.set_current_time(new_time, f64::MAX);
    state.set_time_offset(state.current_time());
    state.start_time = Instant::now();
    let (idx, cumulative) = find_event_index_at_time(cast, state.current_time());
    state.set_event_position(idx, cumulative, cast.events.len());
    state.needs_render = true;
}

/// Handle seeking forward by a given amount.
fn handle_seek_forward(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    amount: f64,
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
) {
    let new_time = (state.current_time() + amount).min(total_duration);
    state.set_current_time(new_time, total_duration);
    state.set_time_offset(state.current_time());
    state.start_time = Instant::now();
    let (idx, cumulative) = find_event_index_at_time(cast, state.current_time());
    state.set_event_position(idx, cumulative, cast.events.len());

    // Rebuild buffer from scratch for forward seek
    *buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);
    let mut cumulative = 0.0f64;
    for event in &cast.events {
        cumulative += event.time;
        if cumulative > state.current_time() {
            break;
        }
        if event.is_output() {
            buffer.process(&event.data, None);
        } else if let Some((cols, rows)) = event.parse_resize() {
            buffer.resize(cols as usize, rows as usize);
        }
    }
    state.needs_render = true;
}

/// Handle seek to start of recording.
fn handle_seek_to_start(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    rec_cols: u32,
    rec_rows: u32,
) {
    seek_to_time(buffer, cast, 0.0, rec_cols, rec_rows);
    state.set_current_time(0.0, f64::MAX);
    state.set_time_offset(0.0);
    state.start_time = Instant::now();
    state.set_event_position(0, 0.0, cast.events.len());
    state.set_view_row_offset(0, 0);
    state.set_view_col_offset(0, 0);
    state.needs_render = true;
}

/// Handle seek to end of recording.
fn handle_seek_to_end(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
) {
    *buffer = TerminalBuffer::new(rec_cols as usize, rec_rows as usize);

    // Process all events
    for event in &cast.events {
        if event.is_output() {
            buffer.process(&event.data, None);
        } else if let Some((cols, rows)) = event.parse_resize() {
            buffer.resize(cols as usize, rows as usize);
        }
    }

    state.set_current_time(total_duration, total_duration);
    state.set_time_offset(state.current_time());
    state.set_event_position(cast.events.len(), total_duration, cast.events.len());
    state.paused = true;
    state.needs_render = true;
}

/// Handle left arrow key (seek or viewport scroll).
fn handle_left_key(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    modifiers: KeyModifiers,
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
) {
    if state.viewport_mode {
        let new_offset = state.view_col_offset().saturating_sub(1);
        state.set_view_col_offset(new_offset, usize::MAX);
        state.needs_render = true;
    } else {
        let step = if modifiers.contains(KeyModifiers::SHIFT) {
            total_duration * 0.05 // 5% jump
        } else {
            5.0 // 5 seconds
        };
        handle_seek_backward(state, buffer, cast, step, rec_cols, rec_rows);
    }
}

/// Handle right arrow key (seek or viewport scroll).
fn handle_right_key(
    state: &mut PlaybackState,
    buffer: &mut TerminalBuffer,
    cast: &AsciicastFile,
    modifiers: KeyModifiers,
    total_duration: f64,
    rec_cols: u32,
    rec_rows: u32,
) {
    if state.viewport_mode {
        let max_offset = buffer.width().saturating_sub(state.view_cols);
        let new_offset = state.view_col_offset() + 1;
        state.set_view_col_offset(new_offset, max_offset);
        state.needs_render = true;
    } else {
        let step = if modifiers.contains(KeyModifiers::SHIFT) {
            total_duration * 0.05 // 5% jump
        } else {
            5.0 // 5 seconds
        };
        handle_seek_forward(
            state,
            buffer,
            cast,
            step,
            total_duration,
            rec_cols,
            rec_rows,
        );
    }
}

/// Handle up arrow key (free mode or viewport scroll).
fn handle_up_key(state: &mut PlaybackState) {
    if state.free_mode {
        // Move highlight up one line
        let old_offset = state.view_row_offset();
        let old_free_line = state.free_line();
        let new_free_line = old_free_line.saturating_sub(1);
        state.set_free_line(new_free_line, usize::MAX);

        // Auto-scroll viewport to keep highlighted line visible
        if state.free_line() < state.view_row_offset() {
            state.set_view_row_offset(state.free_line(), usize::MAX);
        }

        // If viewport didn't scroll, only update highlight lines
        if state.view_row_offset() == old_offset && state.prev_free_line != state.free_line() {
            state.free_line_only = true;
        }
        state.needs_render = true;
    } else if state.viewport_mode {
        let new_offset = state.view_row_offset().saturating_sub(1);
        state.set_view_row_offset(new_offset, usize::MAX);
        state.needs_render = true;
    }
    // In normal mode, up does nothing
}

/// Handle down arrow key (free mode or viewport scroll).
fn handle_down_key(state: &mut PlaybackState, buffer: &TerminalBuffer) {
    if state.free_mode {
        // Move highlight down one line
        let old_offset = state.view_row_offset();
        let max_line = buffer.height().saturating_sub(1);
        let new_free_line = state.free_line() + 1;
        state.set_free_line(new_free_line, max_line);

        // Auto-scroll viewport to keep highlighted line visible
        if state.free_line() >= state.view_row_offset() + state.view_rows {
            let new_offset = state.free_line() - state.view_rows + 1;
            state.set_view_row_offset(new_offset, usize::MAX);
        }

        // If viewport didn't scroll, only update highlight lines
        if state.view_row_offset() == old_offset && state.prev_free_line != state.free_line() {
            state.free_line_only = true;
        }
        state.needs_render = true;
    } else if state.viewport_mode {
        let max_offset = buffer.height().saturating_sub(state.view_rows);
        let new_offset = state.view_row_offset() + 1;
        state.set_view_row_offset(new_offset, max_offset);
        state.needs_render = true;
    }
    // In normal mode, down does nothing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asciicast::{AsciicastFile, Event, Header, TermInfo};

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
        cast.events.push(Event::output(0.1, "hello"));
        cast.events.push(Event::output(0.2, " world"));
        cast.events.push(Event::output(0.3, "!"));
        cast
    }

    fn create_key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn create_key_event_with_mods(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    // === handle_key_event dispatch tests ===

    #[test]
    fn handle_key_event_q_quits() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let result = handle_key_event(
            create_key_event(KeyCode::Char('q')),
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
    fn handle_key_event_ctrl_c_quits() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let result = handle_key_event(
            create_key_event_with_mods(KeyCode::Char('c'), KeyModifiers::CONTROL),
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
    fn handle_key_event_esc_quits_when_no_mode() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let result = handle_key_event(
            create_key_event(KeyCode::Esc),
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
    fn handle_key_event_esc_exits_viewport_mode() {
        let mut state = create_test_state();
        state.viewport_mode = true;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let result = handle_key_event(
            create_key_event(KeyCode::Esc),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert!(!state.viewport_mode);
    }

    #[test]
    fn handle_key_event_esc_exits_free_mode() {
        let mut state = create_test_state();
        state.free_mode = true;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let result = handle_key_event(
            create_key_event(KeyCode::Esc),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert!(!state.free_mode);
    }

    #[test]
    fn handle_key_event_help_closes_on_any_key() {
        let mut state = create_test_state();
        state.show_help = true;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let result = handle_key_event(
            create_key_event(KeyCode::Char('x')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(result, InputResult::Continue);
        assert!(!state.show_help);
    }

    #[test]
    fn handle_key_event_question_toggles_help() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('?')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert!(state.show_help);
    }

    #[test]
    fn handle_key_event_v_toggles_viewport_mode() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('v')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert!(state.viewport_mode);
    }

    #[test]
    fn handle_key_event_f_toggles_free_mode() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('f')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert!(state.free_mode);
        assert!(state.paused);
    }

    #[test]
    fn handle_key_event_space_toggles_pause() {
        let mut state = create_test_state();
        assert!(!state.paused);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char(' ')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert!(state.paused);
    }

    #[test]
    fn handle_key_event_plus_speeds_up() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('+')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.speed, 2.0); // Fixed step from 1.0
    }

    #[test]
    fn handle_key_event_equals_speeds_up() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('=')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.speed, 2.0); // Fixed step from 1.0
    }

    #[test]
    fn handle_key_event_minus_slows_down() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('-')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.speed, 0.5); // Fixed step from 1.0
    }

    #[test]
    fn handle_key_event_underscore_slows_down() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('_')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.speed, 0.5); // Fixed step from 1.0
    }

    #[test]
    fn handle_key_event_home_seeks_to_start() {
        let mut state = create_test_state();
        state.set_current_time(5.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Home),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.current_time(), 0.0);
        assert_eq!(state.view_row_offset(), 0);
        assert_eq!(state.view_col_offset(), 0);
    }

    #[test]
    fn handle_key_event_end_seeks_to_end() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];
        let total_duration = 10.0;

        let _ = handle_key_event(
            create_key_event(KeyCode::End),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            total_duration,
            80,
            24,
        );

        assert_eq!(state.current_time(), total_duration);
        assert!(state.paused);
    }

    #[test]
    fn handle_key_event_less_than_seeks_backward() {
        let mut state = create_test_state();
        state.set_current_time(8.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('<')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.current_time(), 3.0); // 8 - 5 = 3
    }

    #[test]
    fn handle_key_event_comma_seeks_backward() {
        let mut state = create_test_state();
        state.set_current_time(8.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char(',')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.current_time(), 3.0);
    }

    #[test]
    fn handle_key_event_greater_than_seeks_forward() {
        let mut state = create_test_state();
        state.set_current_time(2.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('>')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.current_time(), 7.0); // 2 + 5 = 7
    }

    #[test]
    fn handle_key_event_period_seeks_forward() {
        let mut state = create_test_state();
        state.set_current_time(2.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Char('.')),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.current_time(), 7.0);
    }

    #[test]
    fn handle_key_event_unknown_key_continues() {
        let mut state = create_test_state();
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let result = handle_key_event(
            create_key_event(KeyCode::Char('z')),
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

    // === Arrow key tests ===

    #[test]
    fn handle_key_event_left_seeks_in_normal_mode() {
        let mut state = create_test_state();
        state.set_current_time(8.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Left),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.current_time(), 3.0);
    }

    #[test]
    fn handle_key_event_right_seeks_in_normal_mode() {
        let mut state = create_test_state();
        state.set_current_time(2.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Right),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.current_time(), 7.0);
    }

    #[test]
    fn handle_key_event_left_scrolls_in_viewport_mode() {
        let mut state = create_test_state();
        state.viewport_mode = true;
        state.set_view_col_offset(5, 100);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Left),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            80,
            24,
        );

        assert_eq!(state.view_col_offset(), 4);
    }

    #[test]
    fn handle_key_event_right_scrolls_in_viewport_mode() {
        let mut state = create_test_state();
        state.viewport_mode = true;
        state.set_view_col_offset(5, 100);
        let mut buffer = TerminalBuffer::new(120, 24); // Wider buffer to allow scrolling
        let cast = create_test_cast();
        let markers = vec![];

        let _ = handle_key_event(
            create_key_event(KeyCode::Right),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            10.0,
            120,
            24,
        );

        assert_eq!(state.view_col_offset(), 6);
    }

    #[test]
    fn handle_key_event_shift_left_seeks_5_percent() {
        let mut state = create_test_state();
        state.set_current_time(10.0, 100.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];
        let total_duration = 100.0;

        let _ = handle_key_event(
            create_key_event_with_mods(KeyCode::Left, KeyModifiers::SHIFT),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            total_duration,
            80,
            24,
        );

        assert_eq!(state.current_time(), 5.0); // 10 - (100 * 0.05) = 5
    }

    #[test]
    fn handle_key_event_shift_right_seeks_5_percent() {
        let mut state = create_test_state();
        state.set_current_time(10.0, 100.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![];
        let total_duration = 100.0;

        let _ = handle_key_event(
            create_key_event_with_mods(KeyCode::Right, KeyModifiers::SHIFT),
            &mut state,
            &mut buffer,
            &cast,
            &markers,
            total_duration,
            80,
            24,
        );

        assert_eq!(state.current_time(), 15.0); // 10 + (100 * 0.05) = 15
    }

    // === Up/Down key tests ===

    #[test]
    fn test_handle_up_key_free_mode() {
        let mut state = create_test_state();
        state.free_mode = true;
        state.set_free_line(5, 100);

        handle_up_key(&mut state);

        assert_eq!(state.free_line(), 4);
        assert_eq!(state.prev_free_line, 5);
    }

    #[test]
    fn test_handle_up_key_free_mode_at_top() {
        let mut state = create_test_state();
        state.free_mode = true;
        state.set_free_line(0, 100);

        handle_up_key(&mut state);

        assert_eq!(state.free_line(), 0); // Can't go below 0
    }

    #[test]
    fn test_handle_up_key_free_mode_auto_scroll() {
        let mut state = create_test_state();
        state.free_mode = true;
        state.set_free_line(5, 100);
        state.set_view_row_offset(5, 100);

        handle_up_key(&mut state);

        assert_eq!(state.free_line(), 4);
        assert_eq!(state.view_row_offset(), 4); // Auto-scrolled up
    }

    #[test]
    fn test_handle_up_key_free_mode_sets_free_line_only() {
        let mut state = create_test_state();
        state.free_mode = true;
        state.set_free_line(10, 100);
        state.set_view_row_offset(0, 100); // Viewport at top

        handle_up_key(&mut state);

        assert!(state.free_line_only); // Only line changed, viewport didn't scroll
    }

    #[test]
    fn test_handle_down_key_free_mode() {
        let mut state = create_test_state();
        state.free_mode = true;
        state.set_free_line(5, 100);
        let buffer = TerminalBuffer::new(80, 24);

        handle_down_key(&mut state, &buffer);

        assert_eq!(state.free_line(), 6);
        assert_eq!(state.prev_free_line, 5);
    }

    #[test]
    fn test_handle_down_key_free_mode_at_bottom() {
        let mut state = create_test_state();
        state.free_mode = true;
        state.set_free_line(23, 100); // Last line (0-indexed)
        let buffer = TerminalBuffer::new(80, 24);

        handle_down_key(&mut state, &buffer);

        assert_eq!(state.free_line(), 23); // Can't go past last line
    }

    #[test]
    fn test_handle_down_key_free_mode_auto_scroll() {
        let mut state = create_test_state();
        state.free_mode = true;
        state.set_free_line(23, 100); // Bottom of viewport
        state.view_rows = 24;
        state.set_view_row_offset(0, 100);
        let buffer = TerminalBuffer::new(80, 48); // Buffer taller than viewport

        handle_down_key(&mut state, &buffer);

        assert_eq!(state.free_line(), 24);
        assert_eq!(state.view_row_offset(), 1); // Auto-scrolled down
    }

    #[test]
    fn test_handle_up_key_viewport_mode() {
        let mut state = create_test_state();
        state.viewport_mode = true;
        state.set_view_row_offset(5, 100);

        handle_up_key(&mut state);

        assert_eq!(state.view_row_offset(), 4);
    }

    #[test]
    fn test_handle_up_key_viewport_mode_at_top() {
        let mut state = create_test_state();
        state.viewport_mode = true;
        state.set_view_row_offset(0, 100);

        handle_up_key(&mut state);

        assert_eq!(state.view_row_offset(), 0); // Can't go below 0
    }

    #[test]
    fn test_handle_down_key_viewport_mode() {
        let mut state = create_test_state();
        state.viewport_mode = true;
        state.set_view_row_offset(5, 100);
        let buffer = TerminalBuffer::new(80, 48);

        handle_down_key(&mut state, &buffer);

        assert_eq!(state.view_row_offset(), 6);
    }

    #[test]
    fn test_handle_down_key_viewport_mode_at_bottom() {
        let mut state = create_test_state();
        state.viewport_mode = true;
        state.view_rows = 24;
        state.set_view_row_offset(24, 100); // max offset for 48 rows
        let buffer = TerminalBuffer::new(80, 48);

        handle_down_key(&mut state, &buffer);

        assert_eq!(state.view_row_offset(), 24); // At max
    }

    #[test]
    fn test_handle_up_key_normal_mode_does_nothing() {
        let mut state = create_test_state();
        state.set_view_row_offset(5, 100);

        handle_up_key(&mut state);

        assert_eq!(state.view_row_offset(), 5); // Unchanged
    }

    #[test]
    fn test_handle_down_key_normal_mode_does_nothing() {
        let mut state = create_test_state();
        state.set_view_row_offset(5, 100);
        let buffer = TerminalBuffer::new(80, 48);

        handle_down_key(&mut state, &buffer);

        assert_eq!(state.view_row_offset(), 5); // Unchanged
    }

    // === Seek boundary tests ===

    #[test]
    fn seek_backward_clamps_to_zero() {
        let mut state = create_test_state();
        state.set_current_time(2.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();

        handle_seek_backward(&mut state, &mut buffer, &cast, 5.0, 80, 24);

        assert_eq!(state.current_time(), 0.0);
    }

    #[test]
    fn seek_forward_clamps_to_duration() {
        let mut state = create_test_state();
        state.set_current_time(8.0, 10.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 10.0;

        handle_seek_forward(&mut state, &mut buffer, &cast, 5.0, total_duration, 80, 24);

        assert_eq!(state.current_time(), 10.0);
    }

    // === Marker navigation tests ===

    #[test]
    fn handle_jump_to_marker_jumps_to_next() {
        let mut state = create_test_state();
        state.set_current_time(0.0, 100.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![
            MarkerPosition {
                time: 5.0,
                label: "marker1".to_string(),
            },
            MarkerPosition {
                time: 10.0,
                label: "marker2".to_string(),
            },
        ];

        handle_jump_to_marker(&mut state, &mut buffer, &cast, &markers, 80, 24);

        assert_eq!(state.current_time(), 5.0);
        assert!(state.paused);
    }

    #[test]
    fn handle_jump_to_marker_skips_current() {
        let mut state = create_test_state();
        state.set_current_time(5.0, 100.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![
            MarkerPosition {
                time: 5.0,
                label: "marker1".to_string(),
            },
            MarkerPosition {
                time: 10.0,
                label: "marker2".to_string(),
            },
        ];

        handle_jump_to_marker(&mut state, &mut buffer, &cast, &markers, 80, 24);

        assert_eq!(state.current_time(), 10.0);
    }

    #[test]
    fn handle_jump_to_marker_no_markers_does_nothing() {
        let mut state = create_test_state();
        state.set_current_time(5.0, 100.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers: Vec<MarkerPosition> = vec![];

        handle_jump_to_marker(&mut state, &mut buffer, &cast, &markers, 80, 24);

        assert_eq!(state.current_time(), 5.0); // Unchanged
    }

    #[test]
    fn handle_jump_to_marker_past_last_does_nothing() {
        let mut state = create_test_state();
        state.set_current_time(15.0, 100.0);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let markers = vec![MarkerPosition {
            time: 10.0,
            label: "marker1".to_string(),
        }];

        handle_jump_to_marker(&mut state, &mut buffer, &cast, &markers, 80, 24);

        assert_eq!(state.current_time(), 15.0); // Unchanged
    }

    // === Seek to start/end tests ===

    #[test]
    fn seek_to_start_resets_state() {
        let mut state = create_test_state();
        state.set_current_time(5.0, 100.0);
        state.set_event_position(10, 5.0, 100);
        state.set_view_row_offset(5, 100);
        state.set_view_col_offset(5, 100);
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();

        handle_seek_to_start(&mut state, &mut buffer, &cast, 80, 24);

        assert_eq!(state.current_time(), 0.0);
        assert_eq!(state.time_offset(), 0.0);
        assert_eq!(state.event_idx(), 0);
        assert_eq!(state.cumulative_time(), 0.0);
        assert_eq!(state.view_row_offset(), 0);
        assert_eq!(state.view_col_offset(), 0);
    }

    #[test]
    fn seek_to_end_pauses() {
        let mut state = create_test_state();
        state.paused = false;
        let mut buffer = TerminalBuffer::new(80, 24);
        let cast = create_test_cast();
        let total_duration = 10.0;

        handle_seek_to_end(&mut state, &mut buffer, &cast, total_duration, 80, 24);

        assert!(state.paused);
        assert_eq!(state.current_time(), total_duration);
        assert_eq!(state.event_idx(), cast.events.len());
    }
}
