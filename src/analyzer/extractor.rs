//! Content extraction pipeline orchestrator.
//!
//! The [`ContentExtractor`] coordinates the transform pipeline and creates
//! [`AnalysisSegment`]s from cleaned events.

use crate::asciicast::{Event, Transform};

use super::config::ExtractionConfig;
use super::transforms::{
    BlockTruncator, ContentCleaner, DeduplicateProgressLines, EventCoalescer, FilterEmptyEvents,
    GlobalDeduplicator, NormalizeWhitespace, SimilarityFilter,
};
use super::types::{AnalysisContent, AnalysisSegment, ExtractionStats, TokenEstimator};

/// Extracts analysis content from asciicast events using the transform pipeline.
pub struct ContentExtractor {
    config: ExtractionConfig,
}

impl ContentExtractor {
    /// Create a new content extractor with the given configuration.
    pub fn new(config: ExtractionConfig) -> Self {
        Self { config }
    }

    /// Extract analysis content from events.
    ///
    /// Applies the transform pipeline and creates segments from the cleaned events.
    pub fn extract(&self, events: &mut Vec<Event>) -> AnalysisContent {
        let original_bytes: usize = events.iter().map(|e| e.data.len()).sum();
        let original_event_count = events.len();

        // 1. Basic Cleaning (ANSI, Controls, Visual noise)
        let mut cleaner = ContentCleaner::new(&self.config);
        cleaner.transform(events);

        // 2. Event Coalescing (Rapid, similar events)
        let mut events_coalesced = 0;
        if self.config.coalesce_events {
            let mut coalescer = EventCoalescer::new(
                self.config.similarity_threshold,
                self.config.coalesce_time_threshold,
            );
            coalescer.transform(events);
            events_coalesced = coalescer.coalesced_count();
        }

        // 3. Global Deduplication (Frequent lines & windowed event hashing)
        let (global_lines_deduped, window_events_deduped) = {
            let mut global_deduper = GlobalDeduplicator::new(
                self.config.max_line_repeats,
                self.config.event_window_size,
            );
            global_deduper.transform(events);
            global_deduper.stats()
        };

        // 4. Carriage Return Deduplication
        let mut deduper = DeduplicateProgressLines::new();
        if self.config.dedupe_progress_lines {
            deduper.transform(events);
        }

        // 3. Similarity Filtering (Consecutive redundant lines)
        let mut lines_collapsed = 0;
        if self.config.collapse_similar_lines {
            let mut sim_filter = SimilarityFilter::new(self.config.similarity_threshold);
            sim_filter.transform(events);
            lines_collapsed = sim_filter.collapsed_count();
        }

        // 4. Large Block Truncation
        let mut blocks_truncated = 0;
        if self.config.truncate_large_blocks {
            let mut truncator = BlockTruncator::new(
                self.config.max_block_size,
                self.config.truncation_context_lines,
            );
            truncator.transform(events);
            blocks_truncated = truncator.truncated_count();
        }

        // 5. Final Normalization
        if self.config.normalize_whitespace {
            let mut normalizer = NormalizeWhitespace::new(self.config.max_consecutive_newlines);
            normalizer.transform(events);
        }

        FilterEmptyEvents.transform(events);

        // Calculate stats
        let extracted_bytes: usize = events.iter().map(|e| e.data.len()).sum();
        let stats = ExtractionStats {
            original_bytes,
            extracted_bytes,
            ansi_sequences_stripped: cleaner.ansi_stripped_count(),
            control_chars_stripped: cleaner.control_stripped_count(),
            progress_lines_deduplicated: deduper.deduped_count(),
            events_coalesced,
            global_lines_deduped,
            window_events_deduped,
            lines_collapsed,
            blocks_truncated,
            events_processed: original_event_count,
            events_retained: events.len(),
        };

        // Create segments from events
        self.create_segments(events, stats)
    }

    /// Group events into segments based on time gaps.
    ///
    /// Events in asciicast use relative timestamps (time since previous event).
    /// A new segment starts when an event's relative time exceeds the gap threshold.
    fn create_segments(&self, events: &[Event], stats: ExtractionStats) -> AnalysisContent {
        let estimator = TokenEstimator::default();
        let mut segments = Vec::new();
        let mut current_segment_start = 0;
        let mut current_segment_content = String::new();
        let mut cumulative_time = 0.0;
        let mut segment_start_time = 0.0;

        for (i, event) in events.iter().enumerate() {
            // The event's time field is the gap from the previous event
            let gap = event.time;
            cumulative_time += event.time;

            // Start new segment on significant time gap (if we have content)
            if gap > self.config.segment_time_gap && !current_segment_content.is_empty() {
                let estimated_tokens = estimator.estimate(&current_segment_content);
                segments.push(AnalysisSegment {
                    start_time: segment_start_time,
                    end_time: cumulative_time - gap, // End time is before the gap
                    content: std::mem::take(&mut current_segment_content),
                    estimated_tokens,
                    event_range: (current_segment_start, i),
                });
                current_segment_start = i;
                segment_start_time = cumulative_time;
            }

            if event.is_output() {
                if current_segment_content.is_empty() {
                    segment_start_time = cumulative_time;
                }
                current_segment_content.push_str(&event.data);
            }
        }

        // Don't forget final segment
        if !current_segment_content.is_empty() {
            let estimated_tokens = estimator.estimate(&current_segment_content);
            segments.push(AnalysisSegment {
                start_time: segment_start_time,
                end_time: cumulative_time,
                content: current_segment_content,
                estimated_tokens,
                event_range: (current_segment_start, events.len()),
            });
        }

        let total_tokens = segments.iter().map(|s| s.estimated_tokens).sum();
        let total_duration = cumulative_time;

        AnalysisContent {
            segments,
            total_duration,
            total_tokens,
            stats,
        }
    }
}

impl Default for ContentExtractor {
    fn default() -> Self {
        Self::new(ExtractionConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extractor_creates_segments() {
        let extractor = ContentExtractor::default();
        let mut events = vec![
            Event::output(0.1, "hello\n"),
            Event::output(0.1, "world\n"),
            Event::output(5.0, "after gap\n"), // 5 second gap
        ];

        let content = extractor.extract(&mut events);

        // Should have 2 segments (split by time gap > 2s default threshold)
        assert_eq!(content.segments.len(), 2);
        assert!(content.segments[0].content.contains("hello"));
        assert!(content.segments[1].content.contains("after gap"));
    }

    #[test]
    fn extractor_calculates_stats() {
        let extractor = ContentExtractor::default();
        let mut events = vec![
            Event::output(0.1, "\x1b[31mhello\x1b[0m"),
            Event::output(0.1, " world"),
        ];

        let content = extractor.extract(&mut events);

        assert!(content.stats.ansi_sequences_stripped > 0);
        assert!(content.stats.extracted_bytes < content.stats.original_bytes);
    }

    #[test]
    fn extractor_estimates_tokens() {
        let extractor = ContentExtractor::default();
        let mut events = vec![Event::output(0.1, "hello world this is a test")];

        let content = extractor.extract(&mut events);

        // Token estimate should be reasonable (chars/3 * 0.70)
        assert!(content.total_tokens > 0);
        assert!(content.total_tokens < 100);
    }
}
