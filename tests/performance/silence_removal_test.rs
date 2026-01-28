//! Performance tests for silence removal transform.
//!
//! These tests verify that the transform meets performance requirements:
//! - 100MB file (1M events) in < 5 seconds
//! - O(n) time complexity
//! - O(1) extra memory
//!
//! Run with: `cargo test --test performance`

use std::time::Instant;

use agr::asciicast::{Event, SilenceRemoval, Transform};

// ========================================================================
// Test Infrastructure
// ========================================================================

/// Generate synthetic events for performance testing.
///
/// Creates events with alternating short (0.1s) and long (5.0s) intervals
/// to exercise both the pass-through and clamping paths.
fn generate_synthetic_events(count: usize) -> Vec<Event> {
    (0..count)
        .map(|i| {
            // Alternate between short and long intervals
            let time = if i % 10 == 0 { 5.0 } else { 0.1 };
            // Use varied output to approximate real data
            Event::output(time, format!("output line {}", i))
        })
        .collect()
}

/// Generate approximately 100MB worth of events (~1 million events).
///
/// Each event is roughly 100 bytes when serialized:
/// - time field: ~8 bytes
/// - event type: ~10 bytes
/// - data string: ~20-80 bytes
/// - JSON overhead: ~10 bytes
fn generate_100mb_equivalent() -> Vec<Event> {
    generate_synthetic_events(1_000_000)
}

/// Measure execution time of a transform on the given events.
/// Returns the duration in seconds.
fn measure_transform_time(events: &mut Vec<Event>, threshold: f64) -> f64 {
    let mut transform = SilenceRemoval::new(threshold);
    let start = Instant::now();
    transform.transform(events);
    start.elapsed().as_secs_f64()
}

// ========================================================================
// Performance Tests
// ========================================================================

/// Test: 1 million events transforms in < 5 seconds
///
/// Performance requirement from ADR: 100MB file in < 5 seconds
#[test]
fn one_million_events_under_five_seconds() {
    let mut events = generate_100mb_equivalent();
    assert_eq!(events.len(), 1_000_000);

    let duration = measure_transform_time(&mut events, 2.0);

    println!("1M events transformed in {:.3}s", duration);
    assert!(
        duration < 5.0,
        "Transform took {:.3}s, expected < 5.0s",
        duration
    );
}

/// Test: 10 million events transforms in < 50 seconds (linear scaling)
///
/// Verifies O(n) time complexity - 10x more events should take ~10x more time.
#[test]
fn ten_million_events_linear_scaling() {
    let mut events = generate_synthetic_events(10_000_000);
    assert_eq!(events.len(), 10_000_000);

    let duration = measure_transform_time(&mut events, 2.0);

    println!("10M events transformed in {:.3}s", duration);
    assert!(
        duration < 50.0,
        "Transform took {:.3}s, expected < 50.0s",
        duration
    );
}

/// Test: Memory usage stays bounded (no OOM, no significant allocation)
///
/// The transform should work in-place without allocating additional vectors.
/// We verify this by checking that the algorithm doesn't clone the events vector.
#[test]
fn memory_usage_stays_bounded() {
    // Create events and get their pointer before transform
    let mut events = generate_synthetic_events(100_000);
    let original_ptr = events.as_ptr();
    let original_capacity = events.capacity();

    let mut transform = SilenceRemoval::new(2.0);
    transform.transform(&mut events);

    // Verify the vector wasn't reallocated (same memory location)
    assert_eq!(
        events.as_ptr(),
        original_ptr,
        "Events vector was reallocated - indicates cloning"
    );
    assert_eq!(
        events.capacity(),
        original_capacity,
        "Events vector capacity changed - indicates reallocation"
    );
}

/// Test: No event vector cloning (verify in-place mutation)
///
/// Explicitly verifies the algorithm modifies events in place rather than
/// creating a new vector.
#[test]
fn no_event_vector_cloning() {
    let mut events = vec![
        Event::output(5.0, "long pause"),
        Event::output(0.1, "short"),
        Event::output(10.0, "very long pause"),
    ];

    // Store original addresses to verify in-place mutation
    let original_addrs: Vec<*const Event> = events.iter().map(|e| e as *const Event).collect();

    let mut transform = SilenceRemoval::new(2.0);
    transform.transform(&mut events);

    // Verify same addresses after transform (in-place mutation)
    for (i, event) in events.iter().enumerate() {
        assert_eq!(
            event as *const Event, original_addrs[i],
            "Event {} was moved/cloned instead of mutated in-place",
            i
        );
    }
}

// ========================================================================
// Complexity Verification
// ========================================================================

/// Verify O(n) time: double input = double time (within margin)
///
/// We measure 500K and 1M events, expecting roughly 2x time increase.
/// Allow 50% margin for system variance.
///
/// NOTE: Ignored in CI because very small baselines (2ms) are vulnerable to
/// system noise, creating large ratios that fail on shared runners.
#[test]
#[ignore]
fn verify_linear_time_complexity() {
    // Measure baseline with 500K events
    let mut events_500k = generate_synthetic_events(500_000);
    let time_500k = measure_transform_time(&mut events_500k, 2.0);

    // Measure with 1M events (2x baseline)
    let mut events_1m = generate_synthetic_events(1_000_000);
    let time_1m = measure_transform_time(&mut events_1m, 2.0);

    println!(
        "500K events: {:.3}s, 1M events: {:.3}s, ratio: {:.2}x",
        time_500k,
        time_1m,
        time_1m / time_500k
    );

    // Expect ratio to be between 0.1x and 2.5x (accounting for variance)
    // Lower bound is loose - faster than linear is fine (caching, CPU optimization)
    // Upper bound is strict - slower than O(n) indicates a problem
    let ratio = time_1m / time_500k;
    assert!(
        (0.1..=2.5).contains(&ratio),
        "Time ratio {:.2}x outside expected range [0.1, 2.5] for O(n)",
        ratio
    );
}

/// Verify O(1) space: double input != double memory
///
/// The transform uses constant extra space regardless of input size.
/// We verify this by checking that no new allocations occur during transform.
#[test]
fn verify_constant_space_complexity() {
    // Small input
    let mut events_small = generate_synthetic_events(1_000);
    let ptr_small = events_small.as_ptr();
    let cap_small = events_small.capacity();

    let mut transform = SilenceRemoval::new(2.0);
    transform.transform(&mut events_small);

    // Verify no reallocation for small input
    assert_eq!(events_small.as_ptr(), ptr_small);
    assert_eq!(events_small.capacity(), cap_small);

    // Large input
    let mut events_large = generate_synthetic_events(100_000);
    let ptr_large = events_large.as_ptr();
    let cap_large = events_large.capacity();

    transform.transform(&mut events_large);

    // Verify no reallocation for large input
    assert_eq!(events_large.as_ptr(), ptr_large);
    assert_eq!(events_large.capacity(), cap_large);
}

// ========================================================================
// Scalability Edge Cases
// ========================================================================

/// Test: File with 1 event (no overhead issues)
#[test]
fn single_event_no_overhead() {
    let mut events = vec![Event::output(5.0, "only event")];

    let start = Instant::now();
    let mut transform = SilenceRemoval::new(2.0);
    transform.transform(&mut events);
    let duration = start.elapsed();

    // Should complete in microseconds, not milliseconds
    assert!(
        duration.as_micros() < 1000,
        "Single event took {}us, expected < 1000us",
        duration.as_micros()
    );
    assert!((events[0].time - 2.0).abs() < 0.001);
}

/// Test: File with 100 events (small file fast path)
#[test]
fn hundred_events_fast() {
    let mut events = generate_synthetic_events(100);

    let start = Instant::now();
    let mut transform = SilenceRemoval::new(2.0);
    transform.transform(&mut events);
    let duration = start.elapsed();

    // Should complete in microseconds
    assert!(
        duration.as_micros() < 1000,
        "100 events took {}us, expected < 1000us",
        duration.as_micros()
    );
}

/// Test: File with 10,000 events (medium file)
#[test]
fn ten_thousand_events_reasonable_time() {
    let mut events = generate_synthetic_events(10_000);

    let start = Instant::now();
    let mut transform = SilenceRemoval::new(2.0);
    transform.transform(&mut events);
    let duration = start.elapsed();

    // Should complete in under 100ms
    assert!(
        duration.as_millis() < 100,
        "10K events took {}ms, expected < 100ms",
        duration.as_millis()
    );
}
