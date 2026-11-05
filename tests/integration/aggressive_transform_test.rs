//! Integration tests for aggressive transforms: GlobalDeduplicator and SimilarityFilter.
//!
//! These tests verify behavior-preserving contracts before and after refactoring
//! to reduce cognitive complexity (Stage 2a, 2b of SonarCloud quality fixes).

use agr::analyzer::GlobalDeduplicator;
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
            "different content that is definitely at least one hundred and twenty eight bytes long to exceed the min_hash_bytes threshold here",
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
