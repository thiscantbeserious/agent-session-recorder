//! Result aggregation and marker writing for parallel analysis.
//!
//! This module provides:
//! - `ValidatedMarker` - marker with absolute timestamp and validation
//! - `ResultAggregator` - Builder pattern for collecting and processing chunk results
//! - `MarkerWriter` - integrates with existing `MarkerManager` to write markers
//!
//! # Deduplication Algorithm
//!
//! Overlapping chunks may produce duplicate markers. The deduplication algorithm:
//! 1. Sort all markers by timestamp
//! 2. For markers within 2.0s window with same category, keep only the first
//!
//! # Timestamp Resolution
//!
//! LLM markers use relative timestamps (seconds from chunk start).
//! Resolution formula: `absolute = chunk.time_range.start + marker.relative_timestamp`

use crate::analyzer::backend::{MarkerCategory, RawMarker};
use crate::analyzer::chunk::TimeRange;
use crate::analyzer::worker::ChunkResult;
use crate::asciicast::{AsciicastFile, MarkerManager};
use std::path::Path;

/// Default time window for marker deduplication (seconds).
/// Minimum dedup window as percentage of total duration.
/// 2% means markers must be at least 2% of recording duration apart.
const DEDUP_WINDOW_PERCENT: f64 = 0.02;

/// Minimum absolute dedup window in seconds (floor for short recordings).
const DEDUP_WINDOW_MIN_SECS: f64 = 5.0;

/// Maximum absolute dedup window in seconds (cap for very long recordings).
const DEDUP_WINDOW_MAX_SECS: f64 = 60.0;

/// A validated marker with absolute timestamp.
///
/// Created from `RawMarker` after timestamp resolution and validation.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedMarker {
    /// Absolute timestamp in recording (seconds from start)
    pub timestamp: f64,
    /// Marker label with category prefix
    pub label: String,
    /// Engineering category
    pub category: MarkerCategory,
}

impl ValidatedMarker {
    /// Create a new validated marker.
    pub fn new(timestamp: f64, label: String, category: MarkerCategory) -> Self {
        Self {
            timestamp,
            label,
            category,
        }
    }

    /// Format the marker label with category prefix.
    ///
    /// Format: "[CATEGORY] description"
    pub fn format_label(category: MarkerCategory, description: &str) -> String {
        format!("[{}] {}", category, description)
    }

    /// Get the marker text for writing to cast file.
    pub fn to_marker_text(&self) -> String {
        self.label.clone()
    }
}

/// Information about a failed chunk for error reporting.
#[derive(Debug, Clone)]
pub struct FailedChunkInfo {
    /// Chunk ID
    pub chunk_id: usize,
    /// Error message describing the failure
    pub error: String,
}

/// Report from result aggregation.
#[derive(Debug, Default)]
pub struct AggregationReport {
    /// Total markers collected from all chunks
    pub total_collected: usize,
    /// Markers filtered as invalid
    pub invalid_filtered: usize,
    /// Markers removed by deduplication
    pub duplicates_removed: usize,
    /// Final marker count after processing
    pub final_count: usize,
    /// Failed chunk IDs (for backward compatibility)
    pub failed_chunks: Vec<usize>,
    /// Detailed failure information for each failed chunk
    pub failed_chunk_details: Vec<FailedChunkInfo>,
}

/// Result aggregator for collecting and processing chunk results.
///
/// Uses Builder pattern to configure aggregation behavior.
#[derive(Debug)]
pub struct ResultAggregator {
    /// Time window for deduplication
    dedup_window: f64,
    /// Maximum timestamp (recording duration)
    max_timestamp: f64,
}

impl ResultAggregator {
    /// Create a new result aggregator.
    ///
    /// The dedup window is calculated as a percentage of total duration,
    /// with minimum and maximum bounds.
    pub fn new(max_timestamp: f64) -> Self {
        // Calculate window as percentage of duration
        let window = (max_timestamp * DEDUP_WINDOW_PERCENT)
            .max(DEDUP_WINDOW_MIN_SECS)
            .min(DEDUP_WINDOW_MAX_SECS);

        Self {
            dedup_window: window,
            max_timestamp,
        }
    }

    /// Set custom deduplication window.
    pub fn with_dedup_window(mut self, window: f64) -> Self {
        self.dedup_window = window;
        self
    }

    /// Aggregate results from multiple chunks.
    ///
    /// This method:
    /// 1. Collects markers from all successful chunks
    /// 2. Resolves relative timestamps to absolute
    /// 3. Validates markers (non-empty labels, in-range timestamps)
    /// 4. Deduplicates markers within time window
    /// 5. Sorts by timestamp
    pub fn aggregate(
        &self,
        results: Vec<ChunkResult>,
    ) -> (Vec<ValidatedMarker>, AggregationReport) {
        let mut report = AggregationReport::default();
        let mut all_markers = Vec::new();

        // Collect markers from successful chunks
        for result in results {
            match result.result {
                Ok(raw_markers) => {
                    for raw in raw_markers {
                        report.total_collected += 1;

                        // Resolve timestamp
                        let absolute_ts = resolve_timestamp(&result.time_range, raw.timestamp);

                        // Validate marker
                        if !self.is_valid(&raw, absolute_ts) {
                            report.invalid_filtered += 1;
                            continue;
                        }

                        // Create validated marker with formatted label
                        let label = ValidatedMarker::format_label(raw.category, &raw.label);
                        all_markers.push(ValidatedMarker::new(absolute_ts, label, raw.category));
                    }
                }
                Err(e) => {
                    report.failed_chunks.push(result.chunk_id);
                    report.failed_chunk_details.push(FailedChunkInfo {
                        chunk_id: result.chunk_id,
                        error: format!("{}", e),
                    });
                }
            }
        }

        // Sort by timestamp
        all_markers.sort_by(|a, b| {
            a.timestamp
                .partial_cmp(&b.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Deduplicate
        let before_dedup = all_markers.len();
        let deduplicated = self.deduplicate(all_markers);
        report.duplicates_removed = before_dedup - deduplicated.len();
        report.final_count = deduplicated.len();

        (deduplicated, report)
    }

    /// Check if a marker is valid.
    fn is_valid(&self, raw: &RawMarker, absolute_ts: f64) -> bool {
        // Empty labels are invalid
        if raw.label.trim().is_empty() {
            return false;
        }

        // Negative timestamps are invalid
        if absolute_ts < 0.0 {
            return false;
        }

        // Timestamps beyond recording duration are invalid
        if absolute_ts > self.max_timestamp {
            return false;
        }

        true
    }

    /// Deduplicate markers within time window.
    ///
    /// Algorithm:
    /// 1. Markers must already be sorted by timestamp
    /// 2. For markers within window with same category, keep first only
    fn deduplicate(&self, markers: Vec<ValidatedMarker>) -> Vec<ValidatedMarker> {
        if markers.is_empty() {
            return markers;
        }

        let mut result = Vec::with_capacity(markers.len());
        result.push(markers[0].clone());

        for marker in markers.into_iter().skip(1) {
            let last = result.last().unwrap();

            // Check if within window and same category
            let time_diff = (marker.timestamp - last.timestamp).abs();
            if time_diff < self.dedup_window && marker.category == last.category {
                // Skip duplicate
                continue;
            }

            result.push(marker);
        }

        result
    }
}

/// Resolve relative timestamp to absolute.
///
/// Formula: `absolute = chunk.time_range.start + relative`
pub fn resolve_timestamp(time_range: &TimeRange, relative: f64) -> f64 {
    time_range.start + relative
}

/// Report from marker writing.
#[derive(Debug, Default)]
pub struct WriteReport {
    /// Number of markers written
    pub markers_written: usize,
    /// Whether existing markers were found
    pub had_existing_markers: bool,
    /// Number of existing markers
    pub existing_marker_count: usize,
}

/// Writer for adding markers to cast files.
///
/// Integrates with existing `MarkerManager` for file operations.
pub struct MarkerWriter;

impl MarkerWriter {
    /// Check if a cast file has existing markers.
    pub fn has_existing_markers(cast: &AsciicastFile) -> (bool, usize) {
        let count = cast.events.iter().filter(|e| e.is_marker()).count();
        (count > 0, count)
    }

    /// Write validated markers to a cast file in memory.
    ///
    /// # Arguments
    ///
    /// * `cast` - The cast file to modify
    /// * `markers` - Validated markers to write
    ///
    /// # Returns
    ///
    /// WriteReport with summary of operation.
    pub fn write_markers_to_cast(
        cast: &mut AsciicastFile,
        markers: &[ValidatedMarker],
    ) -> WriteReport {
        let (had_existing, existing_count) = Self::has_existing_markers(cast);

        // Write each marker using MarkerManager
        for marker in markers {
            // MarkerManager::add_marker_to_cast handles positioning
            let _ = MarkerManager::add_marker_to_cast(cast, marker.timestamp, &marker.label);
        }

        WriteReport {
            markers_written: markers.len(),
            had_existing_markers: had_existing,
            existing_marker_count: existing_count,
        }
    }

    /// Write validated markers to a cast file on disk.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the cast file
    /// * `markers` - Validated markers to write
    ///
    /// # Returns
    ///
    /// Result containing WriteReport or error.
    pub fn write_markers<P: AsRef<Path>>(
        path: P,
        markers: &[ValidatedMarker],
    ) -> anyhow::Result<WriteReport> {
        let mut cast = AsciicastFile::parse(path.as_ref())?;
        let report = Self::write_markers_to_cast(&mut cast, markers);
        cast.write(path)?;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::backend::BackendError;
    use crate::asciicast::{Event, Header};
    use std::time::Duration;

    // ============================================
    // ValidatedMarker Tests
    // ============================================

    #[test]
    fn validated_marker_new() {
        let marker = ValidatedMarker::new(
            10.5,
            "[SUCCESS] Test passed".to_string(),
            MarkerCategory::Success,
        );

        assert!((marker.timestamp - 10.5).abs() < 0.001);
        assert_eq!(marker.label, "[SUCCESS] Test passed");
        assert_eq!(marker.category, MarkerCategory::Success);
    }

    #[test]
    fn validated_marker_format_label() {
        assert_eq!(
            ValidatedMarker::format_label(MarkerCategory::Planning, "Started task"),
            "[PLAN] Started task"
        );
        assert_eq!(
            ValidatedMarker::format_label(MarkerCategory::Design, "API design"),
            "[DESIGN] API design"
        );
        assert_eq!(
            ValidatedMarker::format_label(MarkerCategory::Implementation, "Writing code"),
            "[IMPL] Writing code"
        );
        assert_eq!(
            ValidatedMarker::format_label(MarkerCategory::Success, "Tests pass"),
            "[SUCCESS] Tests pass"
        );
        assert_eq!(
            ValidatedMarker::format_label(MarkerCategory::Failure, "Build failed"),
            "[FAILURE] Build failed"
        );
    }

    #[test]
    fn validated_marker_to_marker_text() {
        let marker = ValidatedMarker::new(
            5.0,
            "[IMPL] Created file".to_string(),
            MarkerCategory::Implementation,
        );
        assert_eq!(marker.to_marker_text(), "[IMPL] Created file");
    }

    // ============================================
    // Timestamp Resolution Tests
    // ============================================

    #[test]
    fn resolve_timestamp_from_chunk_start() {
        let time_range = TimeRange::new(100.0, 200.0);

        // Relative timestamp 0.0 = absolute 100.0
        assert!((resolve_timestamp(&time_range, 0.0) - 100.0).abs() < 0.001);
    }

    #[test]
    fn resolve_timestamp_middle_of_chunk() {
        let time_range = TimeRange::new(100.0, 200.0);

        // Relative timestamp 50.0 = absolute 150.0
        assert!((resolve_timestamp(&time_range, 50.0) - 150.0).abs() < 0.001);
    }

    #[test]
    fn resolve_timestamp_at_chunk_end() {
        let time_range = TimeRange::new(100.0, 200.0);

        // Relative timestamp 100.0 = absolute 200.0
        assert!((resolve_timestamp(&time_range, 100.0) - 200.0).abs() < 0.001);
    }

    #[test]
    fn resolve_timestamp_various_chunks() {
        // Different chunk positions
        let test_cases = vec![
            (TimeRange::new(0.0, 100.0), 25.0, 25.0),
            (TimeRange::new(50.0, 150.0), 30.0, 80.0),
            (TimeRange::new(500.0, 600.0), 45.5, 545.5),
        ];

        for (range, relative, expected) in test_cases {
            let absolute = resolve_timestamp(&range, relative);
            assert!(
                (absolute - expected).abs() < 0.001,
                "Expected {} but got {}",
                expected,
                absolute
            );
        }
    }

    // ============================================
    // Single Chunk Aggregation Tests
    // ============================================

    #[test]
    fn aggregate_single_chunk_success() {
        let aggregator = ResultAggregator::new(1000.0);

        let raw_markers = vec![
            RawMarker {
                timestamp: 10.0,
                label: "Started planning".to_string(),
                category: MarkerCategory::Planning,
            },
            RawMarker {
                timestamp: 50.0,
                label: "Tests passed".to_string(),
                category: MarkerCategory::Success,
            },
        ];

        let chunk_result = ChunkResult::success(0, TimeRange::new(100.0, 200.0), raw_markers);

        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 2);
        assert_eq!(report.total_collected, 2);
        assert_eq!(report.final_count, 2);
        assert_eq!(report.invalid_filtered, 0);
        assert_eq!(report.duplicates_removed, 0);

        // Check timestamp resolution
        assert!((markers[0].timestamp - 110.0).abs() < 0.001); // 100 + 10
        assert!((markers[1].timestamp - 150.0).abs() < 0.001); // 100 + 50
    }

    #[test]
    fn aggregate_single_chunk_with_label_formatting() {
        let aggregator = ResultAggregator::new(1000.0);

        let raw_markers = vec![RawMarker {
            timestamp: 5.0,
            label: "Build started".to_string(),
            category: MarkerCategory::Implementation,
        }];

        let chunk_result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), raw_markers);
        let (markers, _) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers[0].label, "[IMPL] Build started");
    }

    // ============================================
    // Multiple Chunks Merge Tests
    // ============================================

    #[test]
    fn aggregate_multiple_chunks_merge_in_order() {
        let aggregator = ResultAggregator::new(1000.0);

        // Chunk 1: timestamps 0-100
        let chunk1_markers = vec![
            RawMarker {
                timestamp: 10.0,
                label: "First marker".to_string(),
                category: MarkerCategory::Planning,
            },
            RawMarker {
                timestamp: 80.0,
                label: "Third marker".to_string(),
                category: MarkerCategory::Implementation,
            },
        ];

        // Chunk 2: timestamps 100-200
        let chunk2_markers = vec![RawMarker {
            timestamp: 20.0, // Absolute: 120.0
            label: "Second marker".to_string(),
            category: MarkerCategory::Design,
        }];

        let results = vec![
            ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk1_markers),
            ChunkResult::success(1, TimeRange::new(100.0, 200.0), chunk2_markers),
        ];

        let (markers, report) = aggregator.aggregate(results);

        assert_eq!(markers.len(), 3);
        assert_eq!(report.total_collected, 3);

        // Verify sorted by timestamp
        assert!((markers[0].timestamp - 10.0).abs() < 0.001);
        assert!((markers[1].timestamp - 80.0).abs() < 0.001);
        assert!((markers[2].timestamp - 120.0).abs() < 0.001);

        // Verify correct markers
        assert!(markers[0].label.contains("First"));
        assert!(markers[1].label.contains("Third"));
        assert!(markers[2].label.contains("Second"));
    }

    #[test]
    fn aggregate_multiple_chunks_handles_failures() {
        let aggregator = ResultAggregator::new(1000.0);

        let chunk1_markers = vec![RawMarker {
            timestamp: 10.0,
            label: "Success marker".to_string(),
            category: MarkerCategory::Success,
        }];

        let results = vec![
            ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk1_markers),
            ChunkResult::failure(
                1,
                TimeRange::new(100.0, 200.0),
                BackendError::Timeout(Duration::from_secs(60)),
            ),
            ChunkResult::success(2, TimeRange::new(200.0, 300.0), vec![]),
        ];

        let (markers, report) = aggregator.aggregate(results);

        assert_eq!(markers.len(), 1);
        assert_eq!(report.failed_chunks, vec![1]);
        // Verify error details are captured
        assert_eq!(report.failed_chunk_details.len(), 1);
        assert_eq!(report.failed_chunk_details[0].chunk_id, 1);
        assert!(report.failed_chunk_details[0].error.contains("timed out"));
    }

    // ============================================
    // Deduplication Tests
    // ============================================

    #[test]
    fn deduplicate_within_window_same_category() {
        let aggregator = ResultAggregator::new(1000.0);

        // Two markers within 2s window with same category
        let chunk_markers = vec![
            RawMarker {
                timestamp: 10.0,
                label: "First attempt".to_string(),
                category: MarkerCategory::Success,
            },
            RawMarker {
                timestamp: 11.5, // 1.5s later, within 2s window
                label: "Duplicate attempt".to_string(),
                category: MarkerCategory::Success,
            },
        ];

        let chunk_result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk_markers);
        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 1);
        assert_eq!(report.duplicates_removed, 1);
        assert!(markers[0].label.contains("First")); // Keep first
    }

    #[test]
    fn no_dedup_different_categories() {
        let aggregator = ResultAggregator::new(1000.0);

        // Two markers within 2s window but different categories
        let chunk_markers = vec![
            RawMarker {
                timestamp: 10.0,
                label: "Success".to_string(),
                category: MarkerCategory::Success,
            },
            RawMarker {
                timestamp: 11.0, // 1s later
                label: "Failure".to_string(),
                category: MarkerCategory::Failure,
            },
        ];

        let chunk_result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk_markers);
        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 2);
        assert_eq!(report.duplicates_removed, 0);
    }

    #[test]
    fn no_dedup_outside_window() {
        // For 1000s recording, window = 1000 * 0.02 = 20s
        let aggregator = ResultAggregator::new(1000.0);

        // Two markers with same category but outside 20s window
        let chunk_markers = vec![
            RawMarker {
                timestamp: 10.0,
                label: "First".to_string(),
                category: MarkerCategory::Planning,
            },
            RawMarker {
                timestamp: 35.0, // 25s later, outside 20s window
                label: "Second".to_string(),
                category: MarkerCategory::Planning,
            },
        ];

        let chunk_result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk_markers);
        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 2);
        assert_eq!(report.duplicates_removed, 0);
    }

    #[test]
    fn dedup_from_overlapping_chunks() {
        let aggregator = ResultAggregator::new(1000.0);

        // Simulating overlapping chunks producing same marker
        // Chunk 1: 0-100, produces marker at absolute 95
        // Chunk 2: 80-180, produces marker at absolute 95 (relative 15)
        let chunk1_markers = vec![RawMarker {
            timestamp: 95.0,
            label: "Feature complete".to_string(),
            category: MarkerCategory::Success,
        }];

        let chunk2_markers = vec![RawMarker {
            timestamp: 15.0, // Will resolve to 80 + 15 = 95
            label: "Feature complete".to_string(),
            category: MarkerCategory::Success,
        }];

        let results = vec![
            ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk1_markers),
            ChunkResult::success(1, TimeRange::new(80.0, 180.0), chunk2_markers),
        ];

        let (markers, report) = aggregator.aggregate(results);

        assert_eq!(markers.len(), 1);
        assert_eq!(report.duplicates_removed, 1);
    }

    #[test]
    fn custom_dedup_window() {
        let aggregator = ResultAggregator::new(1000.0).with_dedup_window(5.0);

        // Two markers 3s apart (within 5s custom window)
        let chunk_markers = vec![
            RawMarker {
                timestamp: 10.0,
                label: "First".to_string(),
                category: MarkerCategory::Planning,
            },
            RawMarker {
                timestamp: 13.0, // 3s later
                label: "Second".to_string(),
                category: MarkerCategory::Planning,
            },
        ];

        let chunk_result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk_markers);
        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 1);
        assert_eq!(report.duplicates_removed, 1);
    }

    // ============================================
    // Invalid Marker Filtering Tests
    // ============================================

    #[test]
    fn filter_empty_label() {
        let aggregator = ResultAggregator::new(1000.0);

        let chunk_markers = vec![
            RawMarker {
                timestamp: 10.0,
                label: "".to_string(), // Empty
                category: MarkerCategory::Success,
            },
            RawMarker {
                timestamp: 20.0,
                label: "   ".to_string(), // Whitespace only
                category: MarkerCategory::Success,
            },
            RawMarker {
                timestamp: 30.0,
                label: "Valid label".to_string(),
                category: MarkerCategory::Success,
            },
        ];

        let chunk_result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk_markers);
        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 1);
        assert_eq!(report.invalid_filtered, 2);
        assert!(markers[0].label.contains("Valid"));
    }

    #[test]
    fn filter_negative_timestamp() {
        let aggregator = ResultAggregator::new(1000.0);

        let chunk_markers = vec![RawMarker {
            timestamp: -5.0, // Results in negative absolute timestamp
            label: "Invalid".to_string(),
            category: MarkerCategory::Planning,
        }];

        let chunk_result = ChunkResult::success(0, TimeRange::new(0.0, 100.0), chunk_markers);
        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 0);
        assert_eq!(report.invalid_filtered, 1);
    }

    #[test]
    fn filter_timestamp_beyond_duration() {
        let aggregator = ResultAggregator::new(100.0); // Recording is only 100s

        let chunk_markers = vec![
            RawMarker {
                timestamp: 50.0, // Absolute: 200 > 100 (recording duration)
                label: "Beyond duration".to_string(),
                category: MarkerCategory::Success,
            },
            RawMarker {
                timestamp: 10.0, // Absolute: 160, still > 100
                label: "Also beyond".to_string(),
                category: MarkerCategory::Success,
            },
        ];

        let chunk_result = ChunkResult::success(0, TimeRange::new(150.0, 250.0), chunk_markers);
        let (markers, report) = aggregator.aggregate(vec![chunk_result]);

        assert_eq!(markers.len(), 0);
        assert_eq!(report.invalid_filtered, 2);
    }

    // ============================================
    // MarkerWriter Tests
    // ============================================

    fn create_test_cast() -> AsciicastFile {
        let mut cast = AsciicastFile::new(Header {
            version: 3,
            width: Some(80),
            height: Some(24),
            term: None,
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

    #[test]
    fn marker_writer_detects_no_existing_markers() {
        let cast = create_test_cast();
        let (has_markers, count) = MarkerWriter::has_existing_markers(&cast);

        assert!(!has_markers);
        assert_eq!(count, 0);
    }

    #[test]
    fn marker_writer_detects_existing_markers() {
        let mut cast = create_test_cast();
        MarkerManager::add_marker_to_cast(&mut cast, 0.15, "existing").unwrap();

        let (has_markers, count) = MarkerWriter::has_existing_markers(&cast);

        assert!(has_markers);
        assert_eq!(count, 1);
    }

    #[test]
    fn marker_writer_writes_markers_to_cast() {
        let mut cast = create_test_cast();

        let markers = vec![
            ValidatedMarker::new(
                0.15,
                "[SUCCESS] Test passed".to_string(),
                MarkerCategory::Success,
            ),
            ValidatedMarker::new(
                0.5,
                "[IMPL] Feature added".to_string(),
                MarkerCategory::Implementation,
            ),
        ];

        let report = MarkerWriter::write_markers_to_cast(&mut cast, &markers);

        assert_eq!(report.markers_written, 2);
        assert!(!report.had_existing_markers);
        assert_eq!(report.existing_marker_count, 0);

        // Verify markers were added
        let marker_count = cast.events.iter().filter(|e| e.is_marker()).count();
        assert_eq!(marker_count, 2);
    }

    #[test]
    fn marker_writer_reports_existing_markers() {
        let mut cast = create_test_cast();
        MarkerManager::add_marker_to_cast(&mut cast, 0.15, "existing").unwrap();

        let markers = vec![ValidatedMarker::new(
            0.5,
            "[PLAN] New marker".to_string(),
            MarkerCategory::Planning,
        )];

        let report = MarkerWriter::write_markers_to_cast(&mut cast, &markers);

        assert_eq!(report.markers_written, 1);
        assert!(report.had_existing_markers);
        assert_eq!(report.existing_marker_count, 1);
    }

    #[test]
    fn marker_writer_empty_markers() {
        let mut cast = create_test_cast();

        let report = MarkerWriter::write_markers_to_cast(&mut cast, &[]);

        assert_eq!(report.markers_written, 0);
        assert!(!report.had_existing_markers);
    }

    // ============================================
    // Integration Tests
    // ============================================

    #[test]
    fn full_aggregation_and_write_pipeline() {
        let mut cast = create_test_cast();

        // Simulate analysis results
        let chunk_markers = vec![
            RawMarker {
                timestamp: 0.05, // Absolute: 0.05
                label: "Started".to_string(),
                category: MarkerCategory::Planning,
            },
            RawMarker {
                timestamp: 0.25, // Absolute: 0.25
                label: "Completed".to_string(),
                category: MarkerCategory::Success,
            },
        ];

        let results = vec![ChunkResult::success(
            0,
            TimeRange::new(0.0, 1.0),
            chunk_markers,
        )];

        // Aggregate
        let aggregator = ResultAggregator::new(1.0);
        let (markers, agg_report) = aggregator.aggregate(results);

        assert_eq!(agg_report.final_count, 2);

        // Write
        let write_report = MarkerWriter::write_markers_to_cast(&mut cast, &markers);

        assert_eq!(write_report.markers_written, 2);

        // Verify file integrity
        let marker_labels: Vec<String> = cast
            .events
            .iter()
            .filter(|e| e.is_marker())
            .map(|e| e.data.clone())
            .collect();

        assert!(marker_labels.iter().any(|l| l.contains("Started")));
        assert!(marker_labels.iter().any(|l| l.contains("Completed")));
    }
}
