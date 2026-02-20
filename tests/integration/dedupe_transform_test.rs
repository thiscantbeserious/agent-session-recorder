//! Behavior-preserving integration tests for DeduplicateProgressLines.
//!
//! These tests replicate and extend the inline tests from
//! `src/analyzer/transforms/dedupe.rs` and establish a green baseline
//! before the Stage 1b complexity-reduction refactor.

use agr::analyzer::DeduplicateProgressLines;
use agr::asciicast::{Event, Transform};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Sum all event times.
fn total_time(events: &[Event]) -> f64 {
    events.iter().map(|e| e.time).sum()
}

/// Collect all output event data as a single joined string.
fn joined_output(events: &[Event]) -> String {
    events
        .iter()
        .filter(|e| e.is_output())
        .map(|e| e.data.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests -- replicate inline tests first, then extend
// ---------------------------------------------------------------------------

/// CR-line collapsing: spinner sequences reduced to the final line.
/// Replicates the `collapses_cr_lines` inline test.
#[test]
fn cr_lines_are_collapsed_to_final_state() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![
        Event::output(0.1, "\r⠋ Building..."),
        Event::output(0.1, "\r⠙ Building..."),
        Event::output(0.1, "\r⠹ Building..."),
        Event::output(0.1, "\r✓ Build complete\n"),
    ];
    deduper.transform(&mut events);

    assert_eq!(
        events.len(),
        1,
        "Spinner sequence should collapse to 1 event"
    );
    assert!(
        events[0].data.contains("Build complete"),
        "Collapsed event must contain final state. data: {:?}",
        events[0].data
    );
}

/// Relative timestamps must be preserved end-to-end.
/// Replicates the `uses_relative_timestamps` inline test.
#[test]
fn relative_timestamps_are_preserved() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![
        Event::output(0.5, "line1\n"),
        Event::output(0.3, "line2\n"),
        Event::output(0.2, "line3\n"),
    ];
    deduper.transform(&mut events);

    assert_eq!(events.len(), 3, "No collapsing, event count unchanged");
    let total: f64 = total_time(&events);
    assert!(
        (total - 1.0).abs() < 0.001,
        "Total time must be 1.0, got {}",
        total
    );
}

/// Time from collapsed CR progress events must carry through to the surviving event.
/// Replicates the `carries_time_from_collapsed_progress` inline test.
#[test]
fn time_carries_through_from_collapsed_progress() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![
        Event::output(0.5, "\rframe1"),
        Event::output(0.5, "\rframe2"),
        Event::output(0.5, "Done\n"),
    ];
    deduper.transform(&mut events);

    assert_eq!(events.len(), 1, "Three progress frames collapse to 1");
    let total: f64 = total_time(&events);
    assert!(
        (total - 1.5).abs() < 0.001,
        "Total time 1.5 must be preserved, got {}",
        total
    );
}

/// Non-output events (markers) must flush any pending CR line first,
/// then pass through with accumulated time applied.
#[test]
fn non_output_event_flushes_pending_state_and_passes_through() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![
        Event::output(0.1, "content\n"),
        Event::marker(0.5, "checkpoint"),
        Event::output(0.1, "after\n"),
    ];
    deduper.transform(&mut events);

    let markers: Vec<_> = events.iter().filter(|e| e.is_marker()).collect();
    assert_eq!(markers.len(), 1, "Marker must survive transform");
    assert_eq!(
        markers[0].data, "checkpoint",
        "Marker label must be preserved"
    );
}

/// CR followed by LF must be treated as a line break (CR+LF = newline),
/// not as an overwrite of the current line.
#[test]
fn cr_followed_by_lf_is_treated_as_newline() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![
        Event::output(0.1, "hello\r\n"),
        Event::output(0.1, "world\r\n"),
    ];
    deduper.transform(&mut events);

    // CR+LF should be a completed line, so "hello" must appear in output
    let all = joined_output(&events);
    assert!(
        all.contains("hello"),
        "CR+LF should emit the line. output: {:?}",
        all
    );
    assert!(
        all.contains("world"),
        "Both lines must be emitted. output: {:?}",
        all
    );
}

/// Trailing content without a terminating newline is flushed at end.
#[test]
fn trailing_content_without_newline_is_flushed() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![Event::output(0.1, "partial content")];
    deduper.transform(&mut events);

    let all = joined_output(&events);
    assert!(
        all.contains("partial content"),
        "Trailing content must be flushed. output: {:?}",
        all
    );
}

/// A CR overwrite discards previous content on the same line.
/// The deduped count must increase for each discarded overwrite.
#[test]
fn cr_overwrite_discards_previous_line_content() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![
        Event::output(0.1, "first"),
        Event::output(0.1, "\rsecond"),
        Event::output(0.1, "\rfinal\n"),
    ];
    deduper.transform(&mut events);

    let all = joined_output(&events);
    // "first" and "second" were overwritten; "final" should survive
    assert!(
        all.contains("final"),
        "Final CR overwrite state should survive. output: {:?}",
        all
    );
    assert!(
        !all.contains("first"),
        "Overwritten content should not appear. output: {:?}",
        all
    );
    assert!(deduper.deduped_count() >= 1, "deduped_count must be >= 1");
}

/// Markers interleaved with CR progress sequences must all survive.
#[test]
fn all_markers_are_preserved_through_cr_sequences() {
    let mut deduper = DeduplicateProgressLines::new();
    let mut events = vec![
        Event::output(0.1, "content\n"),
        Event::marker(0.1, "marker1"),
        Event::output(0.1, "\rprogress1"),
        Event::output(0.1, "\rprogress2"),
        Event::marker(0.1, "marker2"),
        Event::output(0.1, "\rfinal\n"),
        Event::marker(0.1, "marker3"),
    ];

    let original_marker_count = events.iter().filter(|e| e.is_marker()).count();
    deduper.transform(&mut events);
    let final_marker_count = events.iter().filter(|e| e.is_marker()).count();

    assert_eq!(
        original_marker_count, final_marker_count,
        "All {} markers must be preserved",
        original_marker_count
    );
}

/// The total time across all output events must be preserved even when
/// many CR frames are collapsed into a single event.
#[test]
fn total_time_preserved_across_heavy_cr_collapsing() {
    let mut deduper = DeduplicateProgressLines::new();
    let times = [0.1f64, 0.05, 0.05, 0.05, 0.05, 0.2, 0.5];
    let expected_total: f64 = times.iter().sum();

    let mut events = vec![
        Event::output(times[0], "\rframe1"),
        Event::output(times[1], "\rframe2"),
        Event::output(times[2], "\rframe3"),
        Event::output(times[3], "\rframe4"),
        Event::output(times[4], "\rframe5"),
        Event::output(times[5], "\rframe_final\n"),
        Event::output(times[6], "stable line\n"),
    ];
    deduper.transform(&mut events);

    let actual_total: f64 = total_time(&events);
    assert!(
        (actual_total - expected_total).abs() < 0.001,
        "Total time must be preserved: expected {}, got {}",
        expected_total,
        actual_total
    );
}

/// A fresh `DeduplicateProgressLines` must implement `Default` and behave
/// identically to `new()`.
#[test]
fn default_implementation_is_equivalent_to_new() {
    let mut d1 = DeduplicateProgressLines::default();
    let mut d2 = DeduplicateProgressLines::new();

    let events_template = vec![Event::output(0.1, "\rframe"), Event::output(0.1, "done\n")];

    let mut e1 = events_template.clone();
    let mut e2 = events_template;

    d1.transform(&mut e1);
    d2.transform(&mut e2);

    assert_eq!(
        e1.len(),
        e2.len(),
        "default() and new() must behave identically"
    );
    assert_eq!(
        e1.iter().map(|e| &e.data).collect::<Vec<_>>(),
        e2.iter().map(|e| &e.data).collect::<Vec<_>>()
    );
}

/// Pending CR at end of stream (no LF following) should be treated as an
/// overwrite: current line is cleared, nothing emitted for that CR.
#[test]
fn pending_cr_before_non_output_event_clears_current_line() {
    let mut deduper = DeduplicateProgressLines::new();
    // The pending_cr is set by '\r'; if a non-output event follows, the
    // current line should be cleared (treated as overwrite).
    let mut events = vec![
        Event::output(0.1, "some content\r"),
        Event::marker(0.2, "boundary"),
    ];
    deduper.transform(&mut events);

    // Marker must be present regardless
    let markers: Vec<_> = events.iter().filter(|e| e.is_marker()).collect();
    assert_eq!(markers.len(), 1, "Marker must survive pending CR flush");

    // "some content" was overwritten by the pending CR, so it should NOT appear
    let all = joined_output(&events);
    assert!(
        !all.contains("some content"),
        "Content before bare CR should be cleared. output: {:?}",
        all
    );
}
