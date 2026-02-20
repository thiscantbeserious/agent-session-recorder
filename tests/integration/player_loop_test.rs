//! Behavior-preserving tests for `advance_playback()` in player/mod.rs.
//!
//! `run_main_loop()` itself requires a real terminal and event loop, so it
//! cannot be tested directly. The extracted `advance_playback()` function
//! takes explicit parameters and can be tested in isolation.

use agr::asciicast::{AsciicastFile, Event, Header};
use agr::player::{advance_playback, PlaybackState};
use agr::terminal::TerminalBuffer;
use std::time::{Duration, Instant};

/// Build a minimal AsciicastFile from a list of (time, data) output events.
fn make_cast(events: Vec<(f64, &str)>) -> AsciicastFile {
    let header = Header {
        version: 3,
        width: None,
        height: None,
        term: None,
        timestamp: None,
        duration: None,
        title: None,
        command: None,
        env: None,
        idle_time_limit: None,
    };
    let mut cast = AsciicastFile::new(header);
    for (time, data) in events {
        cast.events.push(Event::output(time, data));
    }
    cast
}

/// Set up a `PlaybackState` whose elapsed time reads as `elapsed_secs`.
///
/// We do this by setting `start_time` far in the past and `time_offset` to 0.
fn state_with_elapsed(elapsed_secs: f64) -> PlaybackState {
    let mut state = PlaybackState::new(80, 27);
    // start_time far in the past so that elapsed_secs_f64 ≈ elapsed_secs
    let past = Instant::now()
        .checked_sub(Duration::from_secs_f64(elapsed_secs.max(0.0)))
        .unwrap_or(Instant::now());
    state.start_time = past;
    state
}

// ============================================================================
// advance_playback: basic event processing
// ============================================================================

#[test]
fn advance_playback_processes_events_up_to_elapsed_time() {
    let cast = make_cast(vec![(0.5, "hello"), (1.0, "world"), (2.0, "!")]);
    let mut buffer = TerminalBuffer::new(80, 24);
    let mut state = state_with_elapsed(1.6); // Should process events at 0.5s and 1.5s

    advance_playback(&mut state, &mut buffer, &cast, 10.0);

    // Two events fit within 1.6s (cumulative: 0.5s and 1.5s), third at 3.5s does not
    assert_eq!(state.event_idx(), 2);
}

#[test]
fn advance_playback_sets_needs_render_when_playing() {
    let cast = make_cast(vec![(0.1, "x")]);
    let mut buffer = TerminalBuffer::new(80, 24);
    let mut state = state_with_elapsed(1.0);
    state.needs_render = false;

    advance_playback(&mut state, &mut buffer, &cast, 10.0);

    assert!(
        state.needs_render,
        "needs_render should be set while playing"
    );
}

// ============================================================================
// advance_playback: pause state
// ============================================================================

#[test]
fn advance_playback_is_noop_when_paused() {
    let cast = make_cast(vec![(0.1, "x"), (0.2, "y")]);
    let mut buffer = TerminalBuffer::new(80, 24);
    let mut state = state_with_elapsed(5.0);
    state.paused = true;
    state.needs_render = false;

    advance_playback(&mut state, &mut buffer, &cast, 10.0);

    assert_eq!(state.event_idx(), 0, "paused: event_idx should not advance");
    assert!(
        !state.needs_render,
        "paused: needs_render should not be set"
    );
}

// ============================================================================
// advance_playback: elapsed time capping
// ============================================================================

#[test]
fn advance_playback_caps_elapsed_at_total_duration() {
    let cast = make_cast(vec![(0.5, "a"), (0.5, "b"), (0.5, "c")]);
    let mut buffer = TerminalBuffer::new(80, 24);

    // Set elapsed to far beyond total_duration (1.0s)
    let mut state = state_with_elapsed(100.0);

    advance_playback(&mut state, &mut buffer, &cast, 1.0);

    // current_time must be clamped to total_duration
    assert!(
        state.current_time() <= 1.0,
        "current_time must not exceed total_duration"
    );
}

// ============================================================================
// advance_playback: resize event handling
// ============================================================================

#[test]
fn advance_playback_handles_resize_events() {
    let header = Header {
        version: 3,
        width: None,
        height: None,
        term: None,
        timestamp: None,
        duration: None,
        title: None,
        command: None,
        env: None,
        idle_time_limit: None,
    };
    let mut cast = AsciicastFile::new(header);
    cast.events.push(Event::output(0.1, "before"));
    cast.events
        .push(Event::new(0.2, agr::asciicast::EventType::Resize, "100x40"));

    let mut buffer = TerminalBuffer::new(80, 24);
    let mut state = state_with_elapsed(1.0);

    advance_playback(&mut state, &mut buffer, &cast, 10.0);

    // After resize event, buffer should have new dimensions
    assert_eq!(buffer.width(), 100);
    assert_eq!(buffer.height(), 40);
}

// ============================================================================
// advance_playback: no events processed before their scheduled time
// ============================================================================

#[test]
fn advance_playback_does_not_process_future_events() {
    let cast = make_cast(vec![(5.0, "far future")]);
    let mut buffer = TerminalBuffer::new(80, 24);
    let mut state = state_with_elapsed(1.0); // elapsed < 5.0s

    advance_playback(&mut state, &mut buffer, &cast, 10.0);

    assert_eq!(
        state.event_idx(),
        0,
        "future events should not be processed"
    );
}

// ============================================================================
// PlaybackResult message tests (public API)
// ============================================================================

#[test]
fn playback_result_messages_are_correct() {
    use agr::player::PlaybackResult;

    assert_eq!(
        PlaybackResult::Success("x.cast".to_string()).message(),
        "Played: x.cast"
    );
    assert_eq!(
        PlaybackResult::Interrupted.message(),
        "Playback interrupted"
    );
    assert_eq!(
        PlaybackResult::Error("fail".to_string()).message(),
        "Failed to play: fail"
    );
}
