//! Silence removal transform for asciicast recordings.
//!
//! This module provides the [`SilenceRemoval`] transform that caps event intervals
//! at a configurable threshold. Long pauses in recordings (e.g., user went to lunch)
//! are reduced to the threshold value, making playback more fluid.
//!
//! # Algorithm
//!
//! For each event, if the time since the previous event exceeds the threshold,
//! the interval is capped at the threshold value:
//!
//! ```text
//! if event.time > threshold {
//!     event.time = threshold;
//! }
//! ```
//!
//! # Example
//!
//! ```
//! use agr::asciicast::{Event, SilenceRemoval, Transform};
//!
//! let mut transform = SilenceRemoval::new(2.0);
//! let mut events = vec![
//!     Event::output(0.5, "fast"),
//!     Event::output(1800.0, "after lunch"),  // 30 minute gap
//!     Event::output(0.1, "typing"),
//! ];
//!
//! transform.transform(&mut events);
//!
//! assert!((events[0].time - 0.5).abs() < 0.001);   // unchanged
//! assert!((events[1].time - 2.0).abs() < 0.001);   // clamped from 1800s to 2s
//! assert!((events[2].time - 0.1).abs() < 0.001);   // unchanged
//! ```

use super::{Event, Transform};

/// Default threshold for silence removal (2.0 seconds).
///
/// This value balances:
/// - Long enough to preserve natural reading pauses
/// - Short enough to eliminate "went to get coffee" pauses
/// - Industry common value between aggressive (1s) and conservative (3s)
pub const DEFAULT_SILENCE_THRESHOLD: f64 = 2.0;

/// A transform that caps event intervals at a maximum threshold.
///
/// Events with intervals longer than the threshold have their `time` field
/// reduced to the threshold value. Intervals below the threshold are unchanged.
///
/// # Validation
///
/// The threshold must be a positive, finite number. Construction with invalid
/// values (zero, negative, NaN, infinity) will panic.
///
/// # Performance
///
/// This transform operates in O(n) time and O(1) additional space, making it
/// suitable for large files with millions of events.
#[derive(Debug, Clone)]
pub struct SilenceRemoval {
    threshold: f64,
}

impl SilenceRemoval {
    /// Create a new silence removal transform with the given threshold.
    ///
    /// # Arguments
    ///
    /// * `threshold` - Maximum allowed interval between events in seconds.
    ///   Must be positive and finite.
    ///
    /// # Panics
    ///
    /// Panics if `threshold` is:
    /// - Zero or negative
    /// - NaN
    /// - Infinity
    ///
    /// # Example
    ///
    /// ```
    /// use agr::asciicast::SilenceRemoval;
    ///
    /// let transform = SilenceRemoval::new(2.0);
    /// ```
    pub fn new(threshold: f64) -> Self {
        assert!(
            threshold > 0.0 && threshold.is_finite(),
            "Threshold must be positive and finite, got: {}",
            threshold
        );
        Self { threshold }
    }

    /// Get the configured threshold value.
    pub fn threshold(&self) -> f64 {
        self.threshold
    }
}

impl Transform for SilenceRemoval {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.time > self.threshold {
                event.time = self.threshold;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asciicast::EventType;

    // ========================================================================
    // Behavioral Tests (from ADR scenarios)
    // ========================================================================

    /// Scenario: User went to lunch during recording
    ///
    /// Given a recording where the user took a 30-minute lunch break (1800 second gap)
    /// When silence removal is applied with 2.0s threshold
    /// Then the gap becomes 2.0 seconds
    /// And total recording time drops significantly
    #[test]
    fn user_went_to_lunch_during_recording() {
        let mut events = vec![
            Event::output(0.0, "Starting work..."),
            Event::output(0.5, "Typing code"),
            Event::output(0.3, "More code"),
            Event::output(1800.0, "Back from lunch!"), // 30 minute gap
            Event::output(0.2, "Continuing work"),
        ];

        // Calculate original duration
        let original_duration: f64 = events.iter().map(|e| e.time).sum();
        assert!((original_duration - 1801.0).abs() < 0.001); // ~30 minutes

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // The 1800s gap should now be 2.0s
        assert!((events[3].time - 2.0).abs() < 0.001);

        // Total duration should be drastically reduced
        let new_duration: f64 = events.iter().map(|e| e.time).sum();
        assert!((new_duration - 3.0).abs() < 0.001); // ~3 seconds
    }

    /// Scenario: Rapid CI build output stays untouched
    ///
    /// Given a CI build log with rapid output (0.001s intervals between lines)
    /// When silence removal is applied with any reasonable threshold (e.g., 2.0s)
    /// Then all intervals remain unchanged (all below threshold)
    /// And the recording plays back at original speed
    #[test]
    fn rapid_ci_build_output_stays_untouched() {
        let mut events = vec![
            Event::output(0.001, "Building module 1..."),
            Event::output(0.001, "Building module 2..."),
            Event::output(0.001, "Building module 3..."),
            Event::output(0.001, "Running tests..."),
            Event::output(0.001, "All tests passed!"),
        ];

        let original_times: Vec<f64> = events.iter().map(|e| e.time).collect();

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // All intervals should be unchanged
        for (i, event) in events.iter().enumerate() {
            assert!(
                (event.time - original_times[i]).abs() < 0.0001,
                "Event {} time changed: {} -> {}",
                i,
                original_times[i],
                event.time
            );
        }
    }

    /// Scenario: Mixed typing and thinking
    ///
    /// Given a recording where user types a command (0.1s intervals), thinks for 8 seconds, types more
    /// When silence removal is applied with 2.0s threshold
    /// Then the 8-second thinking pause becomes 2.0 seconds
    /// And the typing rhythm (0.1s intervals) is preserved exactly
    #[test]
    fn mixed_typing_and_thinking() {
        let mut events = vec![
            Event::output(0.1, "l"),
            Event::output(0.1, "s"),
            Event::output(0.1, " "),
            Event::output(0.1, "-"),
            Event::output(0.1, "l"),
            Event::output(0.1, "a"),
            Event::output(8.0, "\n"), // 8 second thinking pause
            Event::output(0.1, "c"),
            Event::output(0.1, "d"),
            Event::output(0.1, " "),
        ];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // Typing rhythm preserved
        assert!((events[0].time - 0.1).abs() < 0.001);
        assert!((events[1].time - 0.1).abs() < 0.001);
        assert!((events[5].time - 0.1).abs() < 0.001);

        // Thinking pause clamped
        assert!((events[6].time - 2.0).abs() < 0.001);

        // More typing preserved
        assert!((events[7].time - 0.1).abs() < 0.001);
        assert!((events[8].time - 0.1).abs() < 0.001);
    }

    /// Scenario: Recording with no long pauses
    ///
    /// Given a fast-paced demo recording where all intervals are under 1 second
    /// When silence removal is applied with 2.0s threshold
    /// Then file content is unchanged (no intervals exceeded threshold)
    #[test]
    fn recording_with_no_long_pauses() {
        let mut events = vec![
            Event::output(0.5, "Hello"),
            Event::output(0.8, " "),
            Event::output(0.3, "World"),
            Event::output(0.9, "!"),
        ];

        let original_times: Vec<f64> = events.iter().map(|e| e.time).collect();

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // All intervals unchanged
        for (i, event) in events.iter().enumerate() {
            assert!(
                (event.time - original_times[i]).abs() < 0.0001,
                "Event {} unexpectedly changed",
                i
            );
        }
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    /// Test: Single event file (still processed correctly)
    #[test]
    fn single_event_file_processed_correctly() {
        let mut events = vec![Event::output(5.0, "Only event")];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // Single event with time > threshold should be clamped
        assert!((events[0].time - 2.0).abs() < 0.001);
        assert_eq!(events.len(), 1);
    }

    /// Test: Empty events list (no panic, no change)
    #[test]
    fn empty_events_list_no_panic() {
        let mut events: Vec<Event> = vec![];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        assert!(events.is_empty());
    }

    /// Test: Very small threshold (0.01s) - aggressive clamping works
    #[test]
    fn very_small_threshold_aggressive_clamping() {
        let mut events = vec![
            Event::output(0.005, "a"),
            Event::output(0.02, "b"),  // > 0.01
            Event::output(0.015, "c"), // > 0.01
            Event::output(0.008, "d"),
        ];

        let mut transform = SilenceRemoval::new(0.01);
        transform.transform(&mut events);

        assert!((events[0].time - 0.005).abs() < 0.0001); // unchanged
        assert!((events[1].time - 0.01).abs() < 0.0001); // clamped
        assert!((events[2].time - 0.01).abs() < 0.0001); // clamped
        assert!((events[3].time - 0.008).abs() < 0.0001); // unchanged
    }

    /// Test: Very large threshold (1000s) - effectively no-op
    #[test]
    fn very_large_threshold_effectively_noop() {
        let mut events = vec![
            Event::output(100.0, "long pause 1"),
            Event::output(500.0, "long pause 2"),
            Event::output(0.1, "quick"),
        ];

        let original_times: Vec<f64> = events.iter().map(|e| e.time).collect();

        let mut transform = SilenceRemoval::new(1000.0);
        transform.transform(&mut events);

        // All intervals unchanged (all below 1000s threshold)
        for (i, event) in events.iter().enumerate() {
            assert!(
                (event.time - original_times[i]).abs() < 0.0001,
                "Event {} unexpectedly changed",
                i
            );
        }
    }

    /// Test: First event with time=0 (unchanged, it's the start)
    #[test]
    fn first_event_with_time_zero_unchanged() {
        let mut events = vec![
            Event::output(0.0, "start"),
            Event::output(0.5, "next"),
            Event::output(3.0, "after pause"),
        ];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // First event time=0 stays 0
        assert!((events[0].time - 0.0).abs() < 0.0001);
        // Second event unchanged (below threshold)
        assert!((events[1].time - 0.5).abs() < 0.0001);
        // Third event clamped
        assert!((events[2].time - 2.0).abs() < 0.0001);
    }

    /// Test: Event exactly at threshold boundary
    #[test]
    fn event_exactly_at_threshold_boundary() {
        let mut events = vec![
            Event::output(2.0, "exactly at threshold"),
            Event::output(2.0000001, "just over threshold"),
            Event::output(1.9999999, "just under threshold"),
        ];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // Exactly at threshold: unchanged (not > threshold)
        assert!((events[0].time - 2.0).abs() < 0.0001);
        // Just over threshold: clamped
        assert!((events[1].time - 2.0).abs() < 0.0001);
        // Just under threshold: unchanged
        assert!((events[2].time - 1.9999999).abs() < 0.0001);
    }

    // ========================================================================
    // Data Integrity Tests
    // ========================================================================

    /// Test: Markers preserved with correct relative timing
    #[test]
    fn markers_preserved_with_correct_timing() {
        let mut events = vec![
            Event::output(0.5, "output"),
            Event::marker(10.0, "important marker"), // long pause before marker
            Event::output(0.3, "more output"),
        ];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // Marker still exists with same label
        assert!(events[1].is_marker());
        assert_eq!(events[1].data, "important marker");
        // Marker time clamped
        assert!((events[1].time - 2.0).abs() < 0.001);
    }

    /// Test: All event types handled (Output, Input, Marker, Resize, Exit)
    #[test]
    fn all_event_types_handled() {
        let mut events = vec![
            Event::output(5.0, "output data"),
            Event::new(5.0, EventType::Input, "user input"),
            Event::marker(5.0, "marker label"),
            Event::new(5.0, EventType::Resize, "80x24"),
            Event::new(5.0, EventType::Exit, "0"),
        ];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // All events should have their time clamped
        for event in &events {
            assert!(
                (event.time - 2.0).abs() < 0.001,
                "Event type {:?} not clamped correctly",
                event.event_type
            );
        }

        // Event types preserved
        assert_eq!(events[0].event_type, EventType::Output);
        assert_eq!(events[1].event_type, EventType::Input);
        assert_eq!(events[2].event_type, EventType::Marker);
        assert_eq!(events[3].event_type, EventType::Resize);
        assert_eq!(events[4].event_type, EventType::Exit);
    }

    /// Test: Unicode content preserved (transform doesn't corrupt data field)
    #[test]
    fn unicode_content_preserved() {
        let unicode_content = "Hello \u{1F600} World \u{4E2D}\u{6587}"; // emoji and Chinese
        let mut events = vec![
            Event::output(5.0, unicode_content),
            Event::marker(5.0, "\u{1F389} celebration"),
        ];

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // Unicode content unchanged
        assert_eq!(events[0].data, unicode_content);
        assert_eq!(events[1].data, "\u{1F389} celebration");
    }

    /// Test: Event order unchanged
    #[test]
    fn event_order_unchanged() {
        let mut events = vec![
            Event::output(0.1, "first"),
            Event::output(5.0, "second"),
            Event::output(0.2, "third"),
            Event::marker(10.0, "fourth"),
            Event::output(0.3, "fifth"),
        ];

        let original_data: Vec<String> = events.iter().map(|e| e.data.clone()).collect();

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        // Order preserved
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.data, original_data[i], "Event {} order changed", i);
        }
    }

    /// Test: Event count unchanged
    #[test]
    fn event_count_unchanged() {
        let mut events = vec![
            Event::output(0.1, "a"),
            Event::output(5.0, "b"),
            Event::output(0.2, "c"),
            Event::marker(10.0, "d"),
            Event::output(0.3, "e"),
        ];

        let original_count = events.len();

        let mut transform = SilenceRemoval::new(2.0);
        transform.transform(&mut events);

        assert_eq!(events.len(), original_count);
    }

    // ========================================================================
    // Composition Tests
    // ========================================================================

    /// Test: Works with TransformChain (multiple transforms in sequence)
    #[test]
    fn works_with_transform_chain() {
        use crate::asciicast::TransformChain;

        let mut events = vec![
            Event::output(0.5, "hello"),
            Event::marker(0.1, "marker"),
            Event::output(10.0, "world"),
        ];

        // Chain silence removal with a marker removal transform
        struct RemoveMarkers;
        impl Transform for RemoveMarkers {
            fn transform(&mut self, events: &mut Vec<Event>) {
                events.retain(|e| !e.is_marker());
            }
        }

        let mut chain = TransformChain::new()
            .with(SilenceRemoval::new(2.0))
            .with(RemoveMarkers);

        chain.transform(&mut events);

        // Silence removed and markers removed
        assert_eq!(events.len(), 2);
        assert!((events[1].time - 2.0).abs() < 0.001);
    }

    /// Test: Can chain two SilenceRemoval transforms (stricter second pass)
    #[test]
    fn can_chain_two_silence_removal_transforms() {
        use crate::asciicast::TransformChain;

        let mut events = vec![
            Event::output(0.1, "fast"),
            Event::output(5.0, "first long pause"),
            Event::output(3.0, "second pause"),
            Event::output(0.2, "quick"),
        ];

        // First pass: cap at 3.0s
        // Second pass: cap at 1.0s
        let mut chain = TransformChain::new()
            .with(SilenceRemoval::new(3.0))
            .with(SilenceRemoval::new(1.0));

        chain.transform(&mut events);

        // After first pass: [0.1, 3.0, 3.0, 0.2]
        // After second pass: [0.1, 1.0, 1.0, 0.2]
        assert!((events[0].time - 0.1).abs() < 0.001);
        assert!((events[1].time - 1.0).abs() < 0.001);
        assert!((events[2].time - 1.0).abs() < 0.001);
        assert!((events[3].time - 0.2).abs() < 0.001);
    }

    // ========================================================================
    // Validation Tests
    // ========================================================================

    /// Test: Reject threshold <= 0 (panic with clear message)
    #[test]
    #[should_panic(expected = "Threshold must be positive and finite")]
    fn reject_zero_threshold() {
        let _ = SilenceRemoval::new(0.0);
    }

    #[test]
    #[should_panic(expected = "Threshold must be positive and finite")]
    fn reject_negative_threshold() {
        let _ = SilenceRemoval::new(-1.0);
    }

    /// Test: Reject NaN threshold
    #[test]
    #[should_panic(expected = "Threshold must be positive and finite")]
    fn reject_nan_threshold() {
        let _ = SilenceRemoval::new(f64::NAN);
    }

    /// Test: Reject Infinity threshold
    #[test]
    #[should_panic(expected = "Threshold must be positive and finite")]
    fn reject_positive_infinity_threshold() {
        let _ = SilenceRemoval::new(f64::INFINITY);
    }

    #[test]
    #[should_panic(expected = "Threshold must be positive and finite")]
    fn reject_negative_infinity_threshold() {
        let _ = SilenceRemoval::new(f64::NEG_INFINITY);
    }

    // ========================================================================
    // API Tests
    // ========================================================================

    /// Test: Default threshold constant value
    #[test]
    fn default_threshold_constant_is_two_seconds() {
        assert!((DEFAULT_SILENCE_THRESHOLD - 2.0).abs() < 0.001);
    }

    /// Test: Can retrieve configured threshold
    #[test]
    fn can_retrieve_configured_threshold() {
        let transform = SilenceRemoval::new(3.5);
        assert!((transform.threshold() - 3.5).abs() < 0.001);
    }

    /// Test: Transform is Clone
    #[test]
    fn transform_is_clone() {
        let transform1 = SilenceRemoval::new(2.5);
        let transform2 = transform1.clone();
        assert!((transform2.threshold() - 2.5).abs() < 0.001);
    }

    /// Test: Transform is Debug
    #[test]
    fn transform_is_debug() {
        let transform = SilenceRemoval::new(2.0);
        let debug_str = format!("{:?}", transform);
        assert!(debug_str.contains("SilenceRemoval"));
        assert!(debug_str.contains("2"));
    }
}
