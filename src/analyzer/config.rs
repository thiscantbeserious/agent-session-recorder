//! Configuration for the content extraction pipeline.

/// Configuration for the content extraction pipeline.
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Strip ANSI escape sequences (always true)
    pub strip_ansi: bool,
    /// Strip control characters (always true)
    pub strip_control_chars: bool,
    /// Deduplicate progress lines using \r
    pub dedupe_progress_lines: bool,
    /// Normalize excessive whitespace
    pub normalize_whitespace: bool,
    /// Maximum consecutive newlines allowed
    pub max_consecutive_newlines: usize,
    /// Strip box drawing characters
    pub strip_box_drawing: bool,
    /// Strip spinner animation characters
    pub strip_spinner_chars: bool,
    /// Strip progress bar block characters
    pub strip_progress_blocks: bool,
    /// Time gap threshold for segment boundaries (seconds)
    pub segment_time_gap: f64,
    /// Enable similarity-based line collapsing (targets redundant log lines)
    pub collapse_similar_lines: bool,
    /// Similarity threshold (0.0 to 1.0) for collapsing lines
    pub similarity_threshold: f64,
    /// Enable coalescing of rapid, similar events (targets TUI redrawing)
    pub coalesce_events: bool,
    /// Time threshold for event coalescing (seconds)
    pub coalesce_time_threshold: f64,
    /// Enable truncation of large output blocks
    pub truncate_large_blocks: bool,
    /// Max times a specific line can repeat globally across the session
    pub max_line_repeats: usize,
    /// Window size for event hashing (number of events to check for redraws)
    pub event_window_size: usize,
    /// Maximum size of an output block before truncation (bytes)
    pub max_block_size: usize,
    /// Number of lines to keep at head/tail during truncation
    pub truncation_context_lines: usize,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            strip_ansi: true,
            strip_control_chars: true,
            dedupe_progress_lines: false,
            normalize_whitespace: true,
            max_consecutive_newlines: 2,
            strip_box_drawing: true,
            strip_spinner_chars: true,
            strip_progress_blocks: true,
            segment_time_gap: 2.0,
            collapse_similar_lines: true,
            similarity_threshold: 0.80,
            coalesce_events: true,
            coalesce_time_threshold: 0.2, // 200ms
            max_line_repeats: 20,
            event_window_size: 50,
            truncate_large_blocks: true,
            max_block_size: 10 * 1024, // 10KB
            truncation_context_lines: 30,
        }
    }
}
