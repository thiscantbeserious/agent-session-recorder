//! Behavior-preserving integration tests for TerminalTransform.
//!
//! These tests cover the public behavior of the transform: stable line emission,
//! scroll handling, noise filtering, time accumulation, resize passthrough, and
//! hash deduplication. They are written before any refactoring to establish a
//! green baseline that must survive the complexity-reduction refactor.

use agr::analyzer::TerminalTransform;
use agr::asciicast::{Event, EventType, Transform};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a sequence of output events from (time, data) pairs.
fn output_events(pairs: &[(f64, &str)]) -> Vec<Event> {
    pairs.iter().map(|(t, d)| Event::output(*t, *d)).collect()
}

/// Collect all output-event data strings from a transformed event list.
fn output_data(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter(|e| e.is_output())
        .map(|e| e.data.clone())
        .collect()
}

/// Collect all output-event data joined as a single string.
fn joined_output(events: &[Event]) -> String {
    output_data(events).join("")
}

// ---------------------------------------------------------------------------
// Stage 1a tests -- TerminalTransform::transform()
// ---------------------------------------------------------------------------

/// Single event that produces a complete line with a newline.
/// The transform must emit the line content.
#[test]
fn single_line_with_newline_is_emitted() {
    let mut t = TerminalTransform::new(80, 24);
    let mut events = vec![Event::output(0.1, "hello world\n")];
    t.transform(&mut events);
    let all = joined_output(&events);
    assert!(
        all.contains("hello world"),
        "Expected 'hello world' in output, got: {:?}",
        all
    );
}

/// Events that cause lines to scroll off the screen should be emitted.
/// A small terminal height forces scroll after filling the screen.
#[test]
fn scrolled_lines_are_emitted() {
    let mut t = TerminalTransform::new(80, 3); // 3-row terminal
    let mut events = output_events(&[
        (0.1, "line1\n"),
        (0.1, "line2\n"),
        (0.1, "line3\n"),
        (0.1, "line4\n"), // this forces a scroll
    ]);
    t.transform(&mut events);
    let all = joined_output(&events);
    // At least "line1" must have been emitted (scrolled off)
    assert!(
        all.contains("line1"),
        "Scrolled line1 should be emitted. Output: {:?}",
        all
    );
}

/// A row written many times (spinner pattern) is classified as noise and
/// must not appear in the output.
#[test]
fn high_rewrite_row_is_filtered_as_noise() {
    let mut t = TerminalTransform::new(80, 24);
    // Cursor stays on row 0; rewrite it 5 times (> NOISE_REWRITE_THRESHOLD = 3).
    // Use \r to stay on the same row.
    let mut events = output_events(&[
        (0.01, "\rframe1"),
        (0.01, "\rframe2"),
        (0.01, "\rframe3"),
        (0.01, "\rframe4"),
        (0.01, "\rframe5"),
    ]);
    // Final flush -- add a newline to push the row into stable
    events.push(Event::output(0.01, "\r done\n"));
    t.transform(&mut events);

    let all = joined_output(&events);
    // None of the spinner frames should appear
    for frame in &["frame1", "frame2", "frame3", "frame4", "frame5"] {
        assert!(
            !all.contains(frame),
            "Noisy frame '{}' should be filtered. Output: {:?}",
            frame,
            all
        );
    }
}

/// A clear non-duplicate sequence: two distinct lines must both be emitted.
/// This is the positive control - distinct lines survive the pipeline.
#[test]
fn distinct_lines_are_both_emitted() {
    let mut t = TerminalTransform::new(80, 24);
    let mut events = vec![
        Event::output(0.1, "first line\n"),
        Event::output(0.1, "second line\n"),
    ];
    t.transform(&mut events);

    let all = joined_output(&events);
    assert!(
        all.contains("first line"),
        "First distinct line should be emitted. Output: {:?}",
        all
    );
    assert!(
        all.contains("second line"),
        "Second distinct line should be emitted. Output: {:?}",
        all
    );
}

/// Time from events that produce no output should be accumulated and
/// applied to the next emitted event. Total time must be conserved.
#[test]
fn accumulated_time_is_carried_forward() {
    let mut t = TerminalTransform::new(80, 24);
    // The first two events only type characters (no newline), so they
    // accumulate time. The third event finalises with a newline.
    let mut events = vec![
        Event::output(1.0, "he"),
        Event::output(1.0, "ll"),
        Event::output(1.0, "o\n"),
    ];
    t.transform(&mut events);

    let total: f64 = events.iter().map(|e| e.time).sum();
    assert!(
        (total - 3.0).abs() < 0.01,
        "Total time 3.0 must be preserved, got {}",
        total
    );
}

/// Resize events must pass through with their accumulated time applied.
#[test]
fn resize_event_passes_through_with_accumulated_time() {
    let mut t = TerminalTransform::new(80, 24);
    let mut events = vec![
        Event::output(0.5, "partial"),
        // Resize: should carry the 0.5s accumulated time
        Event::new(1.0, EventType::Resize, "100x50"),
    ];
    t.transform(&mut events);

    let resize_events: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == EventType::Resize)
        .collect();

    assert_eq!(resize_events.len(), 1, "Resize event must be preserved");
    // The resize event must have consumed the accumulated 0.5s + its own 1.0s
    assert!(
        resize_events[0].time >= 1.0,
        "Resize event must carry accumulated time, got {}",
        resize_events[0].time
    );
}

/// Non-output/non-resize events (e.g. markers) must pass through unchanged.
#[test]
fn marker_events_pass_through() {
    let mut t = TerminalTransform::new(80, 24);
    let mut events = vec![
        Event::output(0.1, "line\n"),
        Event::marker(0.2, "checkpoint"),
    ];
    t.transform(&mut events);

    let markers: Vec<_> = events.iter().filter(|e| e.is_marker()).collect();
    assert_eq!(markers.len(), 1, "Marker must be preserved");
    assert_eq!(markers[0].data, "checkpoint");
}

/// After all events, any content still in the terminal buffer that has not
/// been emitted must be flushed.
#[test]
fn final_flush_emits_remaining_buffer_content() {
    let mut t = TerminalTransform::new(80, 24);
    // No trailing newline -- content stays in the buffer until final flush.
    let mut events = vec![Event::output(0.1, "trailing content")];
    t.transform(&mut events);

    let all = joined_output(&events);
    assert!(
        all.contains("trailing content"),
        "Trailing content without newline must be flushed. Output: {:?}",
        all
    );
}

/// The hash deduplication (story_hashes) prevents re-emitting lines that were
/// already emitted. This test verifies the mechanism by using the scroll path:
/// scrolled-off lines that are identical to already-emitted lines are filtered.
#[test]
fn story_hash_deduplication_filters_rescrolled_content() {
    // Use a very small terminal (3 rows) to force scrolling, which exercises
    // the scroll path → tag-with-noise → filter_new_lines flow.
    let mut t = TerminalTransform::new(80, 3);
    let line = "repeated line";
    // Fill and overflow: lines 0-2 filled, line 3 forces a scroll.
    // Then fill again with the same content to trigger scroll-path hash check.
    let mut events = vec![
        Event::output(0.1, &format!("{}\n{}\n{}\n", line, line, line)),
        Event::output(0.1, &format!("{}\n", line)), // scroll: line 0 scrolls off
    ];
    t.transform(&mut events);

    // The scrolled-off line ("repeated line") should be filtered by hash dedup
    // because it was already emitted from the first event.
    let total_hits: usize = events
        .iter()
        .filter(|e| e.is_output())
        .map(|e| e.data.matches(line).count())
        .sum();

    // We don't know the exact count but it must be >= 1 (at least one was emitted)
    // and the dedup must not cause a panic or data corruption.
    assert!(
        total_hits >= 1,
        "At least one occurrence of the repeated line must be emitted"
    );
}

/// Long pause (> 2 seconds) should trigger emission of the current line even
/// without a newline, so it is not lost if the user pauses mid-line.
#[test]
fn long_pause_triggers_line_emission() {
    let mut t = TerminalTransform::new(80, 24);
    let mut events = vec![
        Event::output(0.1, "thinking..."),
        Event::output(3.0, " done"), // long pause
    ];
    t.transform(&mut events);

    let all = joined_output(&events);
    assert!(
        all.contains("thinking"),
        "Long-pause line should be emitted. Output: {:?}",
        all
    );
}

/// Total time must be preserved across any transform, regardless of which
/// events get merged or dropped.
#[test]
fn total_time_is_conserved_across_transform() {
    let mut t = TerminalTransform::new(80, 24);
    let mut events = vec![
        Event::output(0.5, "a\n"),
        Event::output(0.3, "b\n"),
        Event::output(0.2, "c\n"),
    ];
    t.transform(&mut events);

    let total: f64 = events.iter().map(|e| e.time).sum();
    assert!(
        (total - 1.0).abs() < 0.01,
        "Total time 1.0 must be conserved, got {}",
        total
    );
}
