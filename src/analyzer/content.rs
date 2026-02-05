//! Content cleaning transforms for analysis extraction.
//!
//! This module provides transforms that clean asciicast event data for LLM analysis:
//! - [`ContentCleaner`] - Single-pass state machine for ANSI/control/spinner stripping
//! - [`DeduplicateProgressLines`] - Keeps only final state of `\r`-rewritten lines
//! - [`NormalizeWhitespace`] - Collapses excessive whitespace
//! - [`FilterEmptyEvents`] - Removes events with no remaining content

use std::collections::HashSet;

use crate::asciicast::{Event, Transform};

use super::types::{AnalysisContent, AnalysisSegment, ExtractionStats, TokenEstimator};

// ============================================================================
// Configuration
// ============================================================================

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
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            strip_ansi: true,
            strip_control_chars: true,
            dedupe_progress_lines: true,
            normalize_whitespace: true,
            max_consecutive_newlines: 2,
            strip_box_drawing: true,
            strip_spinner_chars: true,
            strip_progress_blocks: true,
            segment_time_gap: 2.0,
        }
    }
}

// ============================================================================
// ContentCleaner - Single-pass state machine
// ============================================================================

/// State machine states for ANSI sequence parsing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AnsiParseState {
    #[default]
    Normal,
    Escape,    // Saw \x1b
    Csi,       // Saw \x1b[
    CsiParams, // In CSI parameters
    Osc,       // In OSC sequence \x1b]
    OscEscape, // Saw \x1b within OSC (for ST terminator)
}

/// Combined single-pass content cleaner for performance.
///
/// Processes bytes directly using a state machine, avoiding multiple passes
/// and unnecessary allocations. Handles:
/// - ANSI escape sequences (CSI, OSC, simple escapes)
/// - Control characters
/// - Box drawing characters
/// - Spinner animation characters
/// - Progress bar blocks
///
/// **Preserves semantic characters**: `\u{2713}` (checkmark), `\u{2714}` (heavy checkmark),
/// `\u{2715}` (X mark), `\u{26A0}` (warning), `\u{2139}` (info), etc.
pub struct ContentCleaner {
    /// Output buffer, reused across events
    buffer: String,
    /// State machine for ANSI sequence detection
    ansi_state: AnsiParseState,
    /// Characters to strip (visual-only, no semantic meaning)
    strip_chars: HashSet<char>,
    /// Characters with semantic meaning (never strip)
    semantic_chars: HashSet<char>,
    /// Statistics tracking
    ansi_stripped: usize,
    control_stripped: usize,
}

impl ContentCleaner {
    /// Create a new content cleaner with the given configuration.
    pub fn new(config: &ExtractionConfig) -> Self {
        let mut strip_chars = HashSet::new();
        let mut semantic_chars = HashSet::new();

        // Semantic chars - NEVER strip (help LLM identify outcomes)
        for c in [
            '\u{2713}', // ✓ Check mark
            '\u{2714}', // ✔ Heavy check mark
            '\u{2715}', // ✕ Multiplication X
            '\u{26A0}', // ⚠ Warning sign
            '\u{2139}', // ℹ Information source
            '\u{2610}', // ☐ Ballot box
            '\u{2611}', // ☑ Ballot box with check
        ] {
            semantic_chars.insert(c);
        }

        // Box drawing characters (U+2500-U+257F)
        if config.strip_box_drawing {
            for c in '\u{2500}'..='\u{257F}' {
                if !semantic_chars.contains(&c) {
                    strip_chars.insert(c);
                }
            }
            // Also block elements used in box drawing (U+2580-U+259F)
            for c in '\u{2580}'..='\u{259F}' {
                if !semantic_chars.contains(&c) {
                    strip_chars.insert(c);
                }
            }
        }

        // Spinner characters (visual animation only)
        if config.strip_spinner_chars {
            // Claude spinners
            for c in ['\u{273B}', '\u{2733}', '\u{2722}', '\u{2736}', '\u{273D}'] {
                strip_chars.insert(c); // ✻ ✳ ✢ ✶ ✽
            }
            // Gemini braille spinner
            for c in [
                '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}',
                '\u{2827}', '\u{2807}', '\u{280F}',
            ] {
                strip_chars.insert(c); // ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏
            }
            // Visual-only bullets
            for c in ['\u{2022}', '\u{203A}', '\u{25E6}', '\u{22EE}'] {
                strip_chars.insert(c); // • › ◦ ⋮
            }
        }

        // Progress bar blocks
        if config.strip_progress_blocks {
            for c in [
                '\u{2588}', '\u{2591}', '\u{2592}', '\u{2593}', // █ ░ ▒ ▓
                '\u{25BC}', '\u{25B2}', '\u{25CF}', '\u{25CB}', // ▼ ▲ ● ○
            ] {
                strip_chars.insert(c);
            }
        }

        Self {
            buffer: String::with_capacity(4096),
            ansi_state: AnsiParseState::Normal,
            strip_chars,
            semantic_chars,
            ansi_stripped: 0,
            control_stripped: 0,
        }
    }

    /// Process event data in a single pass, returns cleaned string.
    pub fn clean(&mut self, data: &str) -> String {
        self.buffer.clear();

        for c in data.chars() {
            match (&self.ansi_state, c) {
                // ANSI escape start
                (AnsiParseState::Normal, '\x1b') => {
                    self.ansi_state = AnsiParseState::Escape;
                    self.ansi_stripped += 1;
                }
                // CSI sequence start: ESC [
                (AnsiParseState::Escape, '[') => {
                    self.ansi_state = AnsiParseState::Csi;
                }
                // OSC sequence start: ESC ]
                (AnsiParseState::Escape, ']') => {
                    self.ansi_state = AnsiParseState::Osc;
                }
                // Simple escape sequence: ESC followed by single char
                (AnsiParseState::Escape, c) if c.is_ascii_alphabetic() || c == '(' || c == ')' => {
                    // Skip the character after ESC (e.g., ESC c, ESC 7, ESC 8)
                    self.ansi_state = AnsiParseState::Normal;
                }
                // CSI parameter chars
                (AnsiParseState::Csi | AnsiParseState::CsiParams, c)
                    if c.is_ascii_digit() || c == ';' || c == '?' || c == '>' || c == '!' =>
                {
                    self.ansi_state = AnsiParseState::CsiParams;
                }
                // CSI final byte (ends sequence)
                (AnsiParseState::Csi | AnsiParseState::CsiParams, c)
                    if c.is_ascii_alphabetic() || c == '@' || c == '`' =>
                {
                    self.ansi_state = AnsiParseState::Normal;
                }
                // OSC content (consume until BEL or ST)
                (AnsiParseState::Osc, '\x07') => {
                    // BEL terminates OSC
                    self.ansi_state = AnsiParseState::Normal;
                }
                (AnsiParseState::Osc, '\x1b') => {
                    // Possible ST (ESC \) terminator
                    self.ansi_state = AnsiParseState::OscEscape;
                }
                (AnsiParseState::OscEscape, '\\') => {
                    // ST terminator complete
                    self.ansi_state = AnsiParseState::Normal;
                }
                (AnsiParseState::OscEscape, _) => {
                    // Not a valid ST, continue OSC
                    self.ansi_state = AnsiParseState::Osc;
                }
                (AnsiParseState::Osc, _) => {
                    // Inside OSC, skip content
                }
                // Inside any escape sequence - skip
                (AnsiParseState::Escape | AnsiParseState::Csi | AnsiParseState::CsiParams, _) => {
                    // Invalid sequence, reset
                    self.ansi_state = AnsiParseState::Normal;
                }
                // Normal character processing
                (AnsiParseState::Normal, c) => {
                    // Check for control characters (except \t, \n, \r which have meaning)
                    if c < '\x20' && c != '\t' && c != '\n' && c != '\r' {
                        self.control_stripped += 1;
                        continue;
                    }
                    // DEL character
                    if c == '\x7f' {
                        self.control_stripped += 1;
                        continue;
                    }
                    // C1 control characters (0x80-0x9F)
                    if ('\u{0080}'..='\u{009F}').contains(&c) {
                        self.control_stripped += 1;
                        continue;
                    }

                    // Semantic chars are always kept
                    if self.semantic_chars.contains(&c) {
                        self.buffer.push(c);
                        continue;
                    }

                    // Strip configured characters
                    if self.strip_chars.contains(&c) {
                        continue;
                    }

                    // Keep everything else
                    self.buffer.push(c);
                }
            }
        }

        // Handle incomplete sequences (reset state for next event)
        if !matches!(self.ansi_state, AnsiParseState::Normal) {
            self.ansi_state = AnsiParseState::Normal;
        }

        self.buffer.clone()
    }

    /// Get the count of ANSI sequences stripped.
    pub fn ansi_stripped_count(&self) -> usize {
        self.ansi_stripped
    }

    /// Get the count of control characters stripped.
    pub fn control_stripped_count(&self) -> usize {
        self.control_stripped
    }

    /// Reset statistics counters.
    pub fn reset_stats(&mut self) {
        self.ansi_stripped = 0;
        self.control_stripped = 0;
    }
}

impl Transform for ContentCleaner {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                event.data = self.clean(&event.data);
            }
        }
    }
}

// ============================================================================
// DeduplicateProgressLines
// ============================================================================

/// Deduplicates progress lines that use `\r` to overwrite themselves.
///
/// Terminal progress bars often use carriage return (`\r`) to rewrite the same
/// line thousands of times. This transform keeps only the final state of each
/// line, dramatically reducing content size while preserving meaning.
///
/// **Algorithm**:
/// 1. Track "current line buffer" with timestamp of FIRST char
/// 2. When `\r` is encountered, clear buffer but keep timestamp
/// 3. When `\n` is encountered, emit the line with timestamp of line START
/// 4. Non-output events (markers, input) pass through unchanged
pub struct DeduplicateProgressLines {
    current_line: String,
    line_start_time: f64,
    is_progress_line: bool,
    deduped_count: usize,
}

impl DeduplicateProgressLines {
    /// Create a new progress line deduplicator.
    pub fn new() -> Self {
        Self {
            current_line: String::new(),
            line_start_time: 0.0,
            is_progress_line: false,
            deduped_count: 0,
        }
    }

    /// Get the count of deduplicated progress lines.
    pub fn deduped_count(&self) -> usize {
        self.deduped_count
    }
}

impl Default for DeduplicateProgressLines {
    fn default() -> Self {
        Self::new()
    }
}

impl Transform for DeduplicateProgressLines {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output_events = Vec::with_capacity(events.len());

        // Track cumulative time for absolute timestamps
        let mut cumulative_time = 0.0;

        for event in events.drain(..) {
            cumulative_time += event.time;

            // Preserve non-output events (markers, input, resize)
            if !event.is_output() {
                // Emit any pending line content before the marker
                if !self.current_line.is_empty() {
                    output_events.push(Event::output(
                        self.line_start_time,
                        std::mem::take(&mut self.current_line),
                    ));
                }
                output_events.push(event);
                continue;
            }

            for ch in event.data.chars() {
                match ch {
                    '\r' => {
                        // Carriage return: line will be overwritten
                        self.is_progress_line = true;
                        self.current_line.clear();
                        // Update start time to current event time
                        self.line_start_time = cumulative_time;
                    }
                    '\n' => {
                        // Newline: emit current line if not empty
                        if !self.current_line.is_empty() {
                            output_events.push(Event::output(
                                self.line_start_time,
                                format!("{}\n", self.current_line),
                            ));
                        } else {
                            // Emit standalone newline
                            output_events.push(Event::output(cumulative_time, "\n".to_string()));
                        }
                        if self.is_progress_line {
                            self.deduped_count += 1;
                        }
                        self.current_line.clear();
                        self.is_progress_line = false;
                    }
                    _ => {
                        // First char of new line sets the timestamp
                        if self.current_line.is_empty() {
                            self.line_start_time = cumulative_time;
                        }
                        self.current_line.push(ch);
                    }
                }
            }
        }

        // Don't forget trailing content without \n
        if !self.current_line.is_empty() {
            output_events.push(Event::output(
                self.line_start_time,
                std::mem::take(&mut self.current_line),
            ));
        }

        *events = output_events;
    }
}

// ============================================================================
// NormalizeWhitespace
// ============================================================================

/// Normalizes excessive whitespace in event content.
///
/// - Collapses multiple consecutive spaces to a single space
/// - Limits consecutive newlines to a configurable maximum
pub struct NormalizeWhitespace {
    max_consecutive_newlines: usize,
}

impl NormalizeWhitespace {
    /// Create a new whitespace normalizer.
    pub fn new(max_consecutive_newlines: usize) -> Self {
        Self {
            max_consecutive_newlines,
        }
    }
}

impl Default for NormalizeWhitespace {
    fn default() -> Self {
        Self::new(2)
    }
}

impl Transform for NormalizeWhitespace {
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                let mut result = String::with_capacity(event.data.len());
                let mut prev_space = false;
                let mut newline_count = 0;

                for c in event.data.chars() {
                    if c == '\n' {
                        newline_count += 1;
                        if newline_count <= self.max_consecutive_newlines {
                            result.push(c);
                        }
                        prev_space = false;
                    } else if c == ' ' || c == '\t' {
                        newline_count = 0;
                        if !prev_space {
                            result.push(' ');
                            prev_space = true;
                        }
                    } else {
                        newline_count = 0;
                        prev_space = false;
                        result.push(c);
                    }
                }
                event.data = result;
            }
        }
    }
}

// ============================================================================
// FilterEmptyEvents
// ============================================================================

/// Filters out events with no content.
///
/// Removes output events that are empty or contain only whitespace.
/// **Always preserves**: markers, input events, resize events.
pub struct FilterEmptyEvents;

impl Transform for FilterEmptyEvents {
    fn transform(&mut self, events: &mut Vec<Event>) {
        events.retain(|event| {
            // Always keep non-output events (markers, input, resize)
            if !event.is_output() {
                return true;
            }
            // Keep output events only if they have non-whitespace content
            !event.data.trim().is_empty()
        });
    }
}

// ============================================================================
// ContentExtractor - Pipeline Orchestrator
// ============================================================================

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

        // Create and apply transforms
        let mut cleaner = ContentCleaner::new(&self.config);
        cleaner.transform(events);

        let mut deduper = DeduplicateProgressLines::new();
        if self.config.dedupe_progress_lines {
            deduper.transform(events);
        }

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

    // ========================================================================
    // ContentCleaner Tests
    // ========================================================================

    #[test]
    fn content_cleaner_strips_csi_color_codes() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "\x1b[38;5;174mcolored\x1b[0m text";
        let output = cleaner.clean(input);
        assert_eq!(output, "colored text");
    }

    #[test]
    fn content_cleaner_strips_cursor_movement() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "\x1b[2K\x1b[1A\x1b[Ghello";
        let output = cleaner.clean(input);
        assert_eq!(output, "hello");
    }

    #[test]
    fn content_cleaner_strips_osc_sequences() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // OSC terminated by BEL
        let input = "\x1b]0;Window Title\x07visible";
        let output = cleaner.clean(input);
        assert_eq!(output, "visible");

        // OSC terminated by ST (ESC \)
        let input = "\x1b]8;;http://example.com\x1b\\link\x1b]8;;\x1b\\";
        let output = cleaner.clean(input);
        assert_eq!(output, "link");
    }

    #[test]
    fn content_cleaner_strips_control_chars() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // BEL, NUL, and other control chars should be stripped
        let input = "hello\x07\x00world";
        let output = cleaner.clean(input);
        assert_eq!(output, "helloworld");
    }

    #[test]
    fn content_cleaner_preserves_tab_newline_cr() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "hello\tworld\nline2\roverwrite";
        let output = cleaner.clean(input);
        assert_eq!(output, "hello\tworld\nline2\roverwrite");
    }

    #[test]
    fn content_cleaner_preserves_semantic_chars() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // These should NOT be stripped
        let input = "test \u{2713} pass \u{2714} done \u{2715} fail \u{26A0} warn";
        let output = cleaner.clean(input);
        assert!(output.contains('\u{2713}')); // ✓
        assert!(output.contains('\u{2714}')); // ✔
        assert!(output.contains('\u{2715}')); // ✕
        assert!(output.contains('\u{26A0}')); // ⚠
    }

    #[test]
    fn content_cleaner_strips_box_drawing() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "╭───────╮\n│ hello │\n╰───────╯";
        let output = cleaner.clean(input);
        assert_eq!(output, "\n hello \n");
    }

    #[test]
    fn content_cleaner_strips_claude_spinners() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "✻ Thinking... ✳ Working... ✶ Done";
        let output = cleaner.clean(input);
        assert_eq!(output, " Thinking...  Working...  Done");
    }

    #[test]
    fn content_cleaner_strips_gemini_braille_spinners() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "⠋ Loading ⠙ Loading ⠹ Loading";
        let output = cleaner.clean(input);
        assert_eq!(output, " Loading  Loading  Loading");
    }

    #[test]
    fn content_cleaner_strips_progress_blocks() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "Progress: ████░░░░ 50%";
        let output = cleaner.clean(input);
        assert_eq!(output, "Progress:  50%");
    }

    #[test]
    fn content_cleaner_handles_nested_sequences() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // Color inside cursor movement
        let input = "\x1b[2K\x1b[38;5;174mtext\x1b[0m\x1b[1G";
        let output = cleaner.clean(input);
        assert_eq!(output, "text");
    }

    #[test]
    fn content_cleaner_handles_partial_sequences() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // Incomplete CSI at end
        let input = "hello\x1b[";
        let output = cleaner.clean(input);
        assert_eq!(output, "hello");
    }

    // ========================================================================
    // DeduplicateProgressLines Tests
    // ========================================================================

    #[test]
    fn dedupe_progress_collapses_cr_lines() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.1, "\r⠋ Building..."),
            Event::output(0.1, "\r⠙ Building..."),
            Event::output(0.1, "\r⠹ Building..."),
            Event::output(0.1, "\r✓ Build complete\n"),
        ];

        deduper.transform(&mut events);

        // Should have one event with final content
        assert_eq!(events.len(), 1);
        assert!(events[0].data.contains("Build complete"));
    }

    #[test]
    fn dedupe_progress_preserves_markers() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.1, "line1\n"),
            Event::marker(0.1, "marker"),
            Event::output(0.1, "line2\n"),
        ];

        deduper.transform(&mut events);

        // Marker should be preserved in order
        let markers: Vec<_> = events.iter().filter(|e| e.is_marker()).collect();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].data, "marker");
    }

    #[test]
    fn dedupe_progress_preserves_non_progress_lines() {
        let mut deduper = DeduplicateProgressLines::new();
        let mut events = vec![
            Event::output(0.1, "first line\n"),
            Event::output(0.1, "second line\n"),
            Event::output(0.1, "third line\n"),
        ];

        deduper.transform(&mut events);

        // All three lines should be preserved
        let content: String = events.iter().map(|e| e.data.as_str()).collect();
        assert!(content.contains("first line"));
        assert!(content.contains("second line"));
        assert!(content.contains("third line"));
    }

    // ========================================================================
    // NormalizeWhitespace Tests
    // ========================================================================

    #[test]
    fn normalize_collapses_multiple_spaces() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "hello    world")];

        normalizer.transform(&mut events);

        assert_eq!(events[0].data, "hello world");
    }

    #[test]
    fn normalize_limits_consecutive_newlines() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "line1\n\n\n\n\nline2")];

        normalizer.transform(&mut events);

        assert_eq!(events[0].data, "line1\n\nline2");
    }

    #[test]
    fn normalize_converts_tabs_to_space() {
        let mut normalizer = NormalizeWhitespace::new(2);
        let mut events = vec![Event::output(0.1, "hello\t\tworld")];

        normalizer.transform(&mut events);

        assert_eq!(events[0].data, "hello world");
    }

    // ========================================================================
    // FilterEmptyEvents Tests
    // ========================================================================

    #[test]
    fn filter_removes_empty_events() {
        let mut events = vec![
            Event::output(0.1, "hello"),
            Event::output(0.1, ""),
            Event::output(0.1, "world"),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn filter_removes_whitespace_only_events() {
        let mut events = vec![
            Event::output(0.1, "hello"),
            Event::output(0.1, "   \n\t  "),
            Event::output(0.1, "world"),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn filter_preserves_markers() {
        let mut events = vec![
            Event::output(0.1, ""),
            Event::marker(0.1, "marker"),
            Event::output(0.1, ""),
        ];

        FilterEmptyEvents.transform(&mut events);

        assert_eq!(events.len(), 1);
        assert!(events[0].is_marker());
    }

    // ========================================================================
    // ContentExtractor Tests
    // ========================================================================

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

        // Token estimate should be reasonable (chars/4 * 0.85)
        assert!(content.total_tokens > 0);
        assert!(content.total_tokens < 100);
    }
}
