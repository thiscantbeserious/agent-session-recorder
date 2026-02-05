//! Data structures for analysis content extraction.
//!
//! These types represent the cleaned content extracted from asciicast recordings,
//! organized into segments with timestamp ranges and token estimates.

/// A segment of analysis content with time range mapping.
///
/// Created from transformed events for chunking and LLM analysis.
/// Each segment groups consecutive events by time gaps.
#[derive(Debug, Clone)]
pub struct AnalysisSegment {
    /// Start timestamp (absolute, from recording start)
    pub start_time: f64,

    /// End timestamp (absolute)
    pub end_time: f64,

    /// Cleaned text content for this segment
    pub content: String,

    /// Estimated token count for this segment
    pub estimated_tokens: usize,

    /// Range of event indices in original cast file (for reverse mapping)
    pub event_range: (usize, usize),
}

/// Complete analysis content extracted from a cast file.
#[derive(Debug)]
pub struct AnalysisContent {
    /// Segments with time ranges and content
    pub segments: Vec<AnalysisSegment>,

    /// Total recording duration
    pub total_duration: f64,

    /// Total estimated tokens across all segments
    pub total_tokens: usize,

    /// Extraction statistics for transparency
    pub stats: ExtractionStats,
}

impl AnalysisContent {
    /// Find the segment containing a given timestamp.
    pub fn segment_at_time(&self, timestamp: f64) -> Option<&AnalysisSegment> {
        self.segments
            .iter()
            .find(|s| s.start_time <= timestamp && timestamp < s.end_time)
    }

    /// Get segments within a time range (for chunking).
    pub fn segments_in_range(&self, start: f64, end: f64) -> Vec<&AnalysisSegment> {
        self.segments
            .iter()
            .filter(|s| s.end_time > start && s.start_time < end)
            .collect()
    }

    /// Get the combined text content from all segments.
    pub fn text(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Extraction statistics for transparency.
#[derive(Debug, Default, Clone)]
pub struct ExtractionStats {
    /// Original content size in bytes
    pub original_bytes: usize,
    /// Extracted content size in bytes
    pub extracted_bytes: usize,
    /// Number of ANSI sequences stripped
    pub ansi_sequences_stripped: usize,
    /// Number of control characters stripped
    pub control_chars_stripped: usize,
    /// Number of progress lines deduplicated
    pub progress_lines_deduplicated: usize,
    /// Number of events processed
    pub events_processed: usize,
    /// Number of events retained after filtering
    pub events_retained: usize,
}

impl ExtractionStats {
    /// Calculate the compression ratio (percentage of original size retained).
    pub fn compression_ratio(&self) -> f64 {
        if self.original_bytes == 0 {
            return 0.0;
        }
        self.extracted_bytes as f64 / self.original_bytes as f64
    }

    /// Calculate the reduction percentage (how much was removed).
    pub fn reduction_percentage(&self) -> f64 {
        1.0 - self.compression_ratio()
    }
}

/// Estimate token count from text content.
///
/// Uses chars/3 heuristic for terminal content - simple, fast, no dependencies.
/// Applied AFTER cleanup since raw content is 55-89% noise.
#[derive(Debug, Clone)]
pub struct TokenEstimator {
    /// Base ratio: characters per token (default: 4.0)
    chars_per_token: f64,
    /// Safety margin to avoid exceeding limits (default: 0.85 = 15% buffer)
    safety_factor: f64,
}

impl TokenEstimator {
    /// Create a new estimator with custom parameters.
    pub fn new(chars_per_token: f64, safety_factor: f64) -> Self {
        Self {
            chars_per_token,
            safety_factor,
        }
    }

    /// Estimate token count for the given text.
    pub fn estimate(&self, text: &str) -> usize {
        let char_count = text.chars().count();
        let raw_estimate = (char_count as f64 / self.chars_per_token).ceil() as usize;
        (raw_estimate as f64 * self.safety_factor) as usize
    }

    /// Estimate with whitespace bonus (code has more tokens per char).
    ///
    /// Code typically has 3.0-3.5 chars per token due to short identifiers
    /// and many special characters.
    pub fn estimate_code(&self, text: &str) -> usize {
        let char_count = text.chars().count();
        if char_count == 0 {
            return 0;
        }

        let whitespace_count = text.chars().filter(|c| c.is_whitespace()).count();
        let whitespace_ratio = whitespace_count as f64 / char_count as f64;

        // Code-like content has more whitespace (indentation, newlines)
        let adjusted_ratio = if whitespace_ratio > 0.15 {
            3.5 // Code-like content
        } else {
            self.chars_per_token // Prose-like content
        };

        let raw_estimate = (char_count as f64 / adjusted_ratio).ceil() as usize;
        (raw_estimate as f64 * self.safety_factor) as usize
    }
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self {
            // Terminal content tokenizes poorly - short words, symbols, paths
            // Using 3.0 instead of 4.0 is more conservative
            chars_per_token: 3.0,
            // Extra safety buffer for Claude CLI mode which may have
            // additional overhead (system prompt, tool context)
            safety_factor: 0.70, // 30% safety buffer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimator_default_values() {
        let estimator = TokenEstimator::default();
        assert!((estimator.chars_per_token - 3.0).abs() < 0.001);
        assert!((estimator.safety_factor - 0.70).abs() < 0.001);
    }

    #[test]
    fn token_estimator_estimate_basic() {
        let estimator = TokenEstimator::default();

        // 100 chars / 3 = 33.33 tokens * 0.70 = 23.33 -> 23
        let text = "a".repeat(100);
        assert_eq!(estimator.estimate(&text), 23);
    }

    #[test]
    fn token_estimator_estimate_empty() {
        let estimator = TokenEstimator::default();
        assert_eq!(estimator.estimate(""), 0);
    }

    #[test]
    fn token_estimator_estimate_code_with_whitespace() {
        let estimator = TokenEstimator::default();

        // Code with significant whitespace should use 3.5 chars/token
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        let prose = "thisisalongstringwithnowhitespace";

        let code_tokens = estimator.estimate_code(code);
        let prose_tokens = estimator.estimate_code(prose);

        // Code should estimate more tokens per character
        // because it uses the 3.5 ratio instead of 4.0
        assert!(code_tokens > 0);
        assert!(prose_tokens > 0);
    }

    #[test]
    fn extraction_stats_compression_ratio() {
        let stats = ExtractionStats {
            original_bytes: 1000,
            extracted_bytes: 150,
            ..Default::default()
        };

        assert!((stats.compression_ratio() - 0.15).abs() < 0.001);
        assert!((stats.reduction_percentage() - 0.85).abs() < 0.001);
    }

    #[test]
    fn extraction_stats_compression_ratio_zero_original() {
        let stats = ExtractionStats::default();
        assert!((stats.compression_ratio() - 0.0).abs() < 0.001);
    }

    #[test]
    fn analysis_content_segment_at_time() {
        let content = AnalysisContent {
            segments: vec![
                AnalysisSegment {
                    start_time: 0.0,
                    end_time: 10.0,
                    content: "first".to_string(),
                    estimated_tokens: 1,
                    event_range: (0, 5),
                },
                AnalysisSegment {
                    start_time: 10.0,
                    end_time: 20.0,
                    content: "second".to_string(),
                    estimated_tokens: 1,
                    event_range: (5, 10),
                },
            ],
            total_duration: 20.0,
            total_tokens: 2,
            stats: ExtractionStats::default(),
        };

        assert_eq!(content.segment_at_time(5.0).unwrap().content, "first");
        assert_eq!(content.segment_at_time(15.0).unwrap().content, "second");
        assert!(content.segment_at_time(25.0).is_none());
    }

    #[test]
    fn analysis_content_segments_in_range() {
        let content = AnalysisContent {
            segments: vec![
                AnalysisSegment {
                    start_time: 0.0,
                    end_time: 10.0,
                    content: "first".to_string(),
                    estimated_tokens: 1,
                    event_range: (0, 5),
                },
                AnalysisSegment {
                    start_time: 10.0,
                    end_time: 20.0,
                    content: "second".to_string(),
                    estimated_tokens: 1,
                    event_range: (5, 10),
                },
                AnalysisSegment {
                    start_time: 20.0,
                    end_time: 30.0,
                    content: "third".to_string(),
                    estimated_tokens: 1,
                    event_range: (10, 15),
                },
            ],
            total_duration: 30.0,
            total_tokens: 3,
            stats: ExtractionStats::default(),
        };

        // Range overlapping first two segments
        let segments = content.segments_in_range(5.0, 15.0);
        assert_eq!(segments.len(), 2);

        // Range overlapping all three
        let segments = content.segments_in_range(0.0, 30.0);
        assert_eq!(segments.len(), 3);
    }

    #[test]
    fn analysis_content_text() {
        let content = AnalysisContent {
            segments: vec![
                AnalysisSegment {
                    start_time: 0.0,
                    end_time: 10.0,
                    content: "first".to_string(),
                    estimated_tokens: 1,
                    event_range: (0, 5),
                },
                AnalysisSegment {
                    start_time: 10.0,
                    end_time: 20.0,
                    content: "second".to_string(),
                    estimated_tokens: 1,
                    event_range: (5, 10),
                },
            ],
            total_duration: 20.0,
            total_tokens: 2,
            stats: ExtractionStats::default(),
        };

        assert_eq!(content.text(), "first\nsecond");
    }
}
