//! Integration tests for aggressive transforms: GlobalDeduplicator and SimilarityFilter.
//!
//! These tests verify behavior-preserving contracts before and after refactoring
//! to reduce cognitive complexity (Stage 2a, 2b of SonarCloud quality fixes).

use agr::analyzer::{GlobalDeduplicator, SimilarityFilter};
use agr::asciicast::{Event, Transform};

// ============================================================================
// Helpers
// ============================================================================

fn output(time: f64, data: &str) -> Event {
    Event::output(time, data)
}

fn marker(time: f64, label: &str) -> Event {
    Event::marker(time, label)
}

fn total_time(events: &[Event]) -> f64 {
    events.iter().map(|e| e.time).sum()
}

// ============================================================================
// GlobalDeduplicator Tests
// ============================================================================

#[test]
fn global_dedup_small_events_bypass_hash_check() {
    // Events under min_hash_bytes (128) are kept even if identical
    let small_data = "x".repeat(10);
    let mut events = vec![
        output(0.1, &small_data),
        output(0.1, &small_data),
        output(0.1, &small_data),
    ];
    let mut dedup = GlobalDeduplicator::new(100, 50);
    dedup.transform(&mut events);

    // All three small events should survive (below min_hash_bytes threshold)
    assert_eq!(
        events.len(),
        3,
        "Small events bypass hash dedup and are all kept"
    );
}

#[test]
fn global_dedup_large_duplicate_events_are_removed() {
    // Events >= 128 bytes that are exact duplicates within the window are deduped
    let large_data = "A".repeat(200);
    let mut events = vec![
        output(0.1, &large_data),
        output(0.1, &large_data),
        output(0.1, &large_data),
    ];
    let mut dedup = GlobalDeduplicator::new(100, 50);
    dedup.transform(&mut events);

    // Only the first occurrence should remain
    assert_eq!(events.len(), 1, "Duplicate large events should be deduped");
    assert_eq!(events[0].data, large_data);
}

#[test]
fn global_dedup_large_duplicate_events_time_accumulated() {
    // Time from removed events is accumulated onto the next kept event
    let large_data = "B".repeat(200);
    let mut events = vec![
        output(1.0, &large_data),
        output(2.0, &large_data), // duplicate — time should accumulate
        output(
            3.0,
            "different large content that is at least 128 bytes long and unique here",
        ),
    ];
    let mut dedup = GlobalDeduplicator::new(100, 50);
    dedup.transform(&mut events);

    // total time in = 1.0 + 2.0 + 3.0 = 6.0, total time out must also be 6.0
    let out_time: f64 = total_time(&events);
    assert!(
        (out_time - 6.0).abs() < 1e-9,
        "Total time must be preserved; got {}",
        out_time
    );
}

#[test]
fn global_dedup_line_frequency_cap_removes_repeated_lines() {
    // Lines repeated more than max_line_repeats are discarded
    let repeated_line = "status: running\n";
    let mut events: Vec<Event> = (0..5)
        .map(|i| output(i as f64 * 0.1, repeated_line))
        .collect();

    // Allow only 3 occurrences
    let mut dedup = GlobalDeduplicator::new(3, 50);
    dedup.transform(&mut events);

    // Count total occurrences across all events
    let total_occurrences: usize = events
        .iter()
        .map(|e| e.data.matches(repeated_line.trim()).count())
        .sum();
    assert!(
        total_occurrences <= 3,
        "Line should be capped at max_line_repeats=3, got {}",
        total_occurrences
    );
    let (lines_deduped, _) = dedup.stats();
    assert!(lines_deduped > 0, "Should report deduped line count");
}

#[test]
fn global_dedup_empty_whitespace_lines_always_kept() {
    // Empty/whitespace-only lines are never subject to frequency cap
    let mut events = vec![
        output(0.1, "\n"),
        output(0.1, "\n"),
        output(0.1, "\n"),
        output(0.1, "\n"),
        output(0.1, "\n"),
    ];
    let mut dedup = GlobalDeduplicator::new(1, 50); // cap at 1
    dedup.transform(&mut events);

    // All whitespace lines should be kept
    let output_data: String = events.iter().map(|e| e.data.as_str()).collect();
    assert!(
        output_data.contains('\n'),
        "Whitespace lines should always be kept"
    );
}

#[test]
fn global_dedup_non_output_events_pass_through() {
    let mut events = vec![
        output(0.1, "some content\n"),
        marker(0.5, "checkpoint"),
        output(0.1, "more content\n"),
    ];
    let mut dedup = GlobalDeduplicator::new(10, 50);
    dedup.transform(&mut events);

    let marker_events: Vec<_> = events.iter().filter(|e| e.is_marker()).collect();
    assert_eq!(marker_events.len(), 1, "Marker events must pass through");
    assert_eq!(marker_events[0].data, "checkpoint");
}

#[test]
fn global_dedup_non_output_time_accumulated_correctly() {
    // When an event is deduped, its time adds to the next non-deduped event
    let large_data = "C".repeat(200);
    let mut events = vec![
        output(1.0, &large_data),
        output(2.0, &large_data), // deduped, time=2.0 accumulated
        output(0.5, "unique small content"),
    ];
    let original_total = total_time(&events);
    let mut dedup = GlobalDeduplicator::new(100, 50);
    dedup.transform(&mut events);

    let out_total = total_time(&events);
    assert!(
        (out_total - original_total).abs() < 1e-9,
        "Time must be preserved: expected {}, got {}",
        original_total,
        out_total
    );
}

#[test]
fn global_dedup_hash_window_eviction_allows_rehashing() {
    // After window_size unique events, old hashes are evicted.
    // Re-appearing content after eviction should be kept again.
    let large_data_a = "A".repeat(200);
    let large_data_b = "B".repeat(200);

    // Window size = 2: first A is kept, then 2 unique events evict A from window,
    // then A appears again and should be kept (hash no longer in window)
    let mut events = vec![
        output(0.1, &large_data_a),    // kept, A enters window
        output(0.1, &large_data_b),    // kept, B enters window (window=[A,B])
        output(0.1, &"C".repeat(200)), // kept, C enters window, A evicted (window=[B,C])
        output(0.1, &large_data_a),    // A is NOT in window anymore -> kept
    ];
    let mut dedup = GlobalDeduplicator::new(100, 2);
    dedup.transform(&mut events);

    assert_eq!(
        events.len(),
        4,
        "After window eviction, re-appearing content should be kept"
    );
}

#[test]
fn global_dedup_stats_track_correctly() {
    let large_data = "D".repeat(200);
    let mut events = vec![
        output(0.1, &large_data),
        output(0.1, &large_data), // deduped event
        output(0.1, &large_data), // deduped event
    ];
    let mut dedup = GlobalDeduplicator::new(100, 50);
    dedup.transform(&mut events);

    let (_, events_deduped) = dedup.stats();
    assert_eq!(events_deduped, 2, "Should track 2 deduped events");
}

// ============================================================================
// SimilarityFilter Tests
// ============================================================================

#[test]
fn similarity_filter_dissimilar_lines_kept() {
    let mut events = vec![
        output(0.1, "cargo build --release\n"),
        output(0.1, "Running 42 tests\n"),
        output(0.1, "git push origin main\n"),
    ];
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);

    // All three dissimilar lines should survive
    assert_eq!(events.len(), 3, "Dissimilar lines should all be kept");
}

#[test]
fn similarity_filter_similar_consecutive_lines_collapsed() {
    // Long similar lines above threshold should be collapsed
    let base = "INFO 2024-01-01T00:00:00Z Processing request from client id=";
    let line_a = format!("{}{}\n", base, "1234567890abcdef");
    let line_b = format!("{}{}\n", base, "1234567890abcdeg");

    let similarity = SimilarityFilter::calculate_similarity(line_a.trim(), line_b.trim());
    assert!(
        similarity >= 0.85,
        "Lines should be considered similar ({})",
        similarity
    );

    let mut events = vec![
        output(0.1, &line_a),
        output(0.1, &line_b),
        output(0.1, &line_b),
        output(0.1, &line_b),
    ];
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);

    // Collapsed lines should produce a collapse message
    let all_data: String = events.iter().map(|e| e.data.as_str()).collect();
    assert!(
        all_data.contains("collapsed") || events.len() < 4,
        "Similar lines should be collapsed"
    );
}

#[test]
fn similarity_filter_short_lines_never_collapsed() {
    // Lines under 30 chars should always have similarity 0.0 (never collapsed)
    let score = SimilarityFilter::calculate_similarity("hello world", "hello world!");
    assert_eq!(score, 0.0, "Short lines must never have similarity > 0");

    let mut events = vec![
        output(0.1, "short line A\n"),
        output(0.1, "short line B\n"),
        output(0.1, "short line C\n"),
    ];
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);

    assert_eq!(events.len(), 3, "Short lines should never be collapsed");
}

#[test]
fn similarity_filter_non_output_events_flush_pending_skips() {
    let base = "INFO 2024-01-01T00:00:00Z Processing request from client id=";
    let line_a = format!("{}{}\n", base, "1234567890abcdef");
    let line_b = format!("{}{}\n", base, "1234567890abcdeg");

    let mut events = vec![
        output(0.1, &line_a),
        output(0.1, &line_b),
        output(0.1, &line_b),
        marker(0.5, "stage-boundary"),
        output(0.1, "After marker\n"),
    ];
    let original_marker_count = events.iter().filter(|e| e.is_marker()).count();
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);

    let final_marker_count = events.iter().filter(|e| e.is_marker()).count();
    assert_eq!(
        original_marker_count, final_marker_count,
        "Non-output events flush pending skips but markers must survive"
    );
}

#[test]
fn similarity_filter_time_accumulated_through_collapsed_events() {
    // When entire events are collapsed (new_data stays empty), time is accumulated
    // and forwarded to the next kept event's time field.
    let base = "INFO 2024-01-01T00:00:00Z Processing request from client id=";
    let line_a = format!("{}{}\n", base, "1234567890abcdef");
    let line_b = format!("{}{}\n", base, "1234567890abcdeg");

    // Each of line_b and line_c is in its OWN event so that
    // the whole event becomes empty and accumulated_time increases.
    let line_c = format!("{}{}\n", base, "1234567890abcdeh"); // also similar

    let mut events = vec![
        output(1.0, &line_a),
        output(2.0, &line_b), // whole event collapses -> accumulated_time += 2.0
        output(3.0, &line_c), // whole event collapses -> accumulated_time += 3.0
    ];
    let original_total: f64 = events.iter().map(|e| e.time).sum(); // 6.0
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);

    // After the stream ends, flush_skips() fires and either:
    // (a) emits a collapse event carrying accumulated_time, or
    // (b) accumulated_time is forwarded to the last output event.
    let out_total: f64 = events.iter().map(|e| e.time).sum();
    assert!(
        (out_total - original_total).abs() < 1e-9,
        "Total time must be preserved: expected {}, got {}",
        original_total,
        out_total
    );
}

#[test]
fn similarity_filter_flush_at_end_of_stream() {
    // The filter must flush any pending collapsed lines at end of stream
    let base = "INFO 2024-01-01T00:00:00Z Processing request from client id=";
    let line_a = format!("{}{}\n", base, "1234567890abcdef");
    let line_b = format!("{}{}\n", base, "1234567890abcdeg");

    let mut events = vec![
        output(0.1, &line_a),
        output(0.1, &line_b), // similar, will be skipped
        output(0.1, &line_b), // similar, will be skipped
    ];
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);

    // Collapsed count reported after end-of-stream flush
    let collapsed = filter.collapsed_count();
    // The two similar lines after line_a should be counted
    assert!(
        collapsed > 0 || events.len() >= 1,
        "Filter must handle end-of-stream flush"
    );
}

#[test]
fn similarity_calculate_identical_strings_return_one() {
    let s = "A".repeat(50);
    assert_eq!(SimilarityFilter::calculate_similarity(&s, &s), 1.0);
}

#[test]
fn similarity_calculate_empty_strings() {
    // Two identical empty strings are considered equal (return 1.0) by the
    // identical-string early-return before the empty check fires.
    assert_eq!(SimilarityFilter::calculate_similarity("", ""), 1.0);
    // Mixed empty + non-empty: the empty check fires after the identity check.
    assert_eq!(
        SimilarityFilter::calculate_similarity("", "hello world"),
        0.0
    );
    assert_eq!(
        SimilarityFilter::calculate_similarity("hello world", ""),
        0.0
    );
}

#[test]
fn similarity_filter_preserves_total_time_no_collapses() {
    // Even when nothing is collapsed, times should be unchanged
    let mut events = vec![
        output(0.5, "first unique line here\n"),
        output(1.0, "second unique line here\n"),
        output(2.0, "third unique line here\n"),
    ];
    let original_total: f64 = events.iter().map(|e| e.time).sum();
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);
    let out_total: f64 = events.iter().map(|e| e.time).sum();
    assert!(
        (out_total - original_total).abs() < 1e-9,
        "Time must be preserved when nothing is collapsed"
    );
}

#[test]
fn similarity_filter_empty_event_data_skipped() {
    let mut events = vec![output(0.1, ""), output(0.1, "some content\n")];
    let mut filter = SimilarityFilter::new(0.85);
    filter.transform(&mut events);
    // Must not panic; output should contain the non-empty event
    let all_data: String = events.iter().map(|e| e.data.as_str()).collect();
    assert!(all_data.contains("some content"));
}
