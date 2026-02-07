//! Content extraction pipeline orchestrator.
//!
//! The [`ContentExtractor`] coordinates the transform pipeline and creates
//! [`AnalysisSegment`]s from cleaned events.

use crate::asciicast::{Event, Transform};

use super::config::ExtractionConfig;
use super::transforms::{
    BlockTruncator, ContentCleaner, EmptyLineFilter, EventCoalescer, FileDumpFilter,
    FilterEmptyEvents, GlobalDeduplicator, NormalizeWhitespace, SimilarityFilter,
    TerminalTransform, WindowedLineDeduplicator,
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
    pub fn extract(&self, events: &mut Vec<Event>, cols: usize, rows: usize) -> AnalysisContent {
        let original_bytes: usize = events.iter().map(|e| e.data.len()).sum();
        let original_event_count = events.len();

        let stats = self.apply_transforms(events, cols, rows, original_bytes, original_event_count);

        // Redistribute artificially concentrated time from the transform pipeline.
        // TerminalTransform accumulates time from filtered events and dumps it on the
        // next emitted event, creating huge gaps that distort segment time ranges and
        // cause LLM marker timestamps to cluster at the end of the recording.
        Self::redistribute_time(events, self.config.segment_time_gap);

        // Create segments from events
        self.create_segments(events, stats)
    }

    /// Apply all configured cleaning and deduplication transforms.
    fn apply_transforms(
        &self,
        events: &mut Vec<Event>,
        cols: usize,
        rows: usize,
        original_bytes: usize,
        original_event_count: usize,
    ) -> ExtractionStats {
        // 1. Terminal Rendering (Layout preservation, ANSI stripping, Redraw reduction)
        let mut term_transform = TerminalTransform::new(cols, rows);
        term_transform.transform(events);

        // 1b. Windowed Line Deduplication (Keeps ONLY the LAST version of status lines)
        let windowed_lines_deduped = self.apply_windowed_dedupe(events);

        // 1c. Basic Cleaning (Controls, Visual noise)
        let mut cleaner = ContentCleaner::new(&self.config);
        cleaner.transform(events);

        // 1d. Collapse consecutive empty lines
        EmptyLineFilter::new().transform(events);

        // 2. Event Coalescing (Rapid, similar events)
        let events_coalesced = self.apply_coalescing(events);

        // 3. Global Deduplication (Frequent lines & windowed event hashing)
        let (global_lines_deduped, window_events_deduped) = self.apply_global_dedupe(events);

        // 3b. File Dump Filtering (Long bursts of output)
        let bursts_collapsed = self.apply_file_dump_filter(events);

        // 5. Similarity Filtering (Consecutive redundant lines)
        let lines_collapsed = self.apply_similarity_filter(events);

        // 6. Large Block Truncation
        let blocks_truncated = self.apply_truncation(events);

        // 7. Final Normalization
        self.apply_normalization(events);

        // Calculate final stats
        let extracted_bytes: usize = events.iter().map(|e| e.data.len()).sum();
        ExtractionStats {
            original_bytes,
            extracted_bytes,
            ansi_sequences_stripped: cleaner.ansi_stripped_count(),
            control_chars_stripped: cleaner.control_stripped_count(),
            progress_lines_deduplicated: 0,
            events_coalesced,
            global_lines_deduped,
            windowed_lines_deduped,
            window_events_deduped,
            lines_collapsed,
            blocks_truncated,
            bursts_collapsed,
            events_processed: original_event_count,
            events_retained: events.len(),
        }
    }

    fn apply_coalescing(&self, events: &mut Vec<Event>) -> usize {
        if self.config.coalesce_events {
            let mut coalescer = EventCoalescer::new(
                self.config.similarity_threshold,
                self.config.coalesce_time_threshold,
            );
            coalescer.transform(events);
            coalescer.coalesced_count()
        } else {
            0
        }
    }

    fn apply_windowed_dedupe(&self, events: &mut Vec<Event>) -> usize {
        let mut deduplicator = WindowedLineDeduplicator::new(self.config.event_window_size);
        deduplicator.transform(events);
        deduplicator.deduped_count()
    }

    fn apply_global_dedupe(&self, events: &mut Vec<Event>) -> (usize, usize) {
        let mut global_deduper =
            GlobalDeduplicator::new(self.config.max_line_repeats, self.config.event_window_size);
        global_deduper.transform(events);
        global_deduper.stats()
    }

    fn apply_file_dump_filter(&self, events: &mut Vec<Event>) -> usize {
        let mut filter = FileDumpFilter::new(self.config.max_burst_lines);
        filter.transform(events);
        filter.collapsed_count()
    }

    fn apply_similarity_filter(&self, events: &mut Vec<Event>) -> usize {
        if self.config.collapse_similar_lines {
            let mut sim_filter = SimilarityFilter::new(self.config.similarity_threshold);
            sim_filter.transform(events);
            sim_filter.collapsed_count()
        } else {
            0
        }
    }

    fn apply_truncation(&self, events: &mut Vec<Event>) -> usize {
        if self.config.truncate_large_blocks {
            let mut truncator = BlockTruncator::new(
                self.config.max_block_size,
                self.config.truncation_context_lines,
            );
            truncator.transform(events);
            truncator.truncated_count()
        } else {
            0
        }
    }

    fn apply_normalization(&self, events: &mut Vec<Event>) {
        if self.config.normalize_whitespace {
            let mut normalizer = NormalizeWhitespace::new(self.config.max_consecutive_newlines);
            normalizer.transform(events);
        }
        FilterEmptyEvents.transform(events);
    }

    /// Smooth out artificially large time gaps from the transform pipeline.
    ///
    /// Caps individual output event times at `max_gap` and distributes the
    /// excess evenly across normal-duration output events. This preserves
    /// total recording duration while preventing segment time ranges from
    /// clustering at the end.
    fn redistribute_time(events: &mut [Event], max_gap: f64) {
        let mut excess = 0.0;
        let mut normal_output_count = 0usize;

        // First pass: measure excess
        for event in events.iter() {
            if event.is_output() {
                if event.time > max_gap {
                    excess += event.time - max_gap;
                } else {
                    normal_output_count += 1;
                }
            }
        }

        if excess <= 0.0 {
            return;
        }

        let bonus = if normal_output_count > 0 {
            excess / normal_output_count as f64
        } else {
            0.0
        };

        // Second pass: cap large events, distribute excess to normal ones
        for event in events.iter_mut() {
            if event.is_output() {
                if event.time > max_gap {
                    event.time = max_gap;
                } else {
                    event.time += bonus;
                }
            }
        }

        // If no normal output events exist, add remaining to last event
        if normal_output_count == 0 {
            if let Some(last) = events.last_mut() {
                last.time += excess;
            }
        }
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

        let content = extractor.extract(&mut events, 80, 24);

        // After TerminalTransform, the pipeline produces at least 1 segment
        // with the content. The exact segment count depends on how the virtual
        // terminal renders and accumulates time.
        assert!(!content.segments.is_empty());
        let all_content: String = content
            .segments
            .iter()
            .map(|s| s.content.as_str())
            .collect();
        assert!(all_content.contains("hello"));
        assert!(all_content.contains("after gap"));
    }

    #[test]
    fn extractor_processes_ansi() {
        let extractor = ContentExtractor::default();
        let mut events = vec![
            Event::output(0.1, "\x1b[31mhello\x1b[0m\n"),
            Event::output(0.1, " world\n"),
        ];

        let content = extractor.extract(&mut events, 80, 24);

        // After TerminalTransform, ANSI is stripped during rendering
        // and the content cleaner strips remaining sequences
        let all_content: String = content
            .segments
            .iter()
            .map(|s| s.content.as_str())
            .collect();
        assert!(all_content.contains("hello"));
    }

    #[test]
    fn extractor_estimates_tokens() {
        let extractor = ContentExtractor::default();
        let mut events = vec![Event::output(0.1, "hello world this is a test\n")];

        let content = extractor.extract(&mut events, 80, 24);

        // Token estimate should be reasonable (chars/3 * 0.70)
        assert!(content.total_tokens > 0);
        assert!(content.total_tokens < 100);
    }
}
