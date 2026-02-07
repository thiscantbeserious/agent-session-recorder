//! Terminal emulation transform.
//!
//! Uses a virtual terminal buffer to process events, handling ANSI escape
//! sequences and carriage return overwrites correctly. This produces a
//! "rendered" version of the terminal state, which is much cleaner for
//! TUI sessions and preserves spatial layout (indentation).
//!
//! Noise detection uses two layers:
//! 1. **Behavioral**: Tracks how many times each terminal row is rewritten.
//!    Rows with high rewrite counts (spinners, progress bars, status bars)
//!    are classified as noise without examining content.
//! 2. **Structural fallback**: [`super::noise::NoiseClassifier`] catches
//!    one-shot noise (tips, hints, update banners) that appear exactly once.

use super::noise::NoiseClassifier;
use crate::asciicast::{Event, EventType, Transform};
use crate::terminal::TerminalBuffer;
use std::collections::{HashSet, VecDeque};
use std::hash::{Hash, Hasher};

/// Maximum number of line hashes to retain. Limits memory for long sessions
/// while still catching redraws within a ~50K-line window. Each entry is 8
/// bytes, so 50 000 entries ≈ 400 KB.
const MAX_STORY_HASHES: usize = 50_000;

/// Minimum number of writes to a terminal row before its content is
/// classified as noise. Normal content writes each row once; spinners and
/// status bars rewrite the same row many times.
const NOISE_REWRITE_THRESHOLD: usize = 3;

/// A transform that renders events through a virtual terminal and extracts
/// a clean, deduplicated chronological "story" of the session.
pub struct TerminalTransform {
    buffer: TerminalBuffer,
    /// Number of stable lines already emitted from the current buffer state
    stable_lines_count: usize,
    /// Last cursor position to detect and skip typing increments
    last_cursor_pos: (usize, usize),
    /// Hashes of lines already included in the stable story to prevent duplicates from redraws
    story_hashes: HashSet<u64>,
    /// Insertion order for FIFO eviction of story_hashes
    story_hash_order: VecDeque<u64>,
    /// Per-row write counter. Indexed by terminal row; length = terminal height.
    /// Rows with count >= NOISE_REWRITE_THRESHOLD are considered noise (spinners,
    /// progress bars, status bars that rewrite in-place).
    row_write_counts: Vec<usize>,
}

impl TerminalTransform {
    /// Create a new terminal transform with given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            buffer: TerminalBuffer::new(width, height),
            stable_lines_count: 0,
            last_cursor_pos: (0, 0),
            story_hashes: HashSet::with_capacity(MAX_STORY_HASHES),
            story_hash_order: VecDeque::with_capacity(MAX_STORY_HASHES),
            row_write_counts: vec![0; height],
        }
    }

    /// Returns `true` if the given row has been rewritten enough times to be
    /// considered noise (behavioral detection).
    fn is_noisy_row(&self, row: usize) -> bool {
        self.row_write_counts.get(row).copied().unwrap_or(0) >= NOISE_REWRITE_THRESHOLD
    }

    /// Shift row_write_counts after `n` lines scrolled off the top.
    fn shift_row_counts(&mut self, n: usize) {
        let drain = n.min(self.row_write_counts.len());
        self.row_write_counts.drain(0..drain);
        self.row_write_counts.resize(self.buffer.height(), 0);
    }

    fn hash_line(line: &str) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // We trim trailing whitespace for hashing to treat redraws with different
        // padding as identical, but we preserve leading whitespace for indentation.
        line.trim_end().hash(&mut hasher);
        hasher.finish()
    }

    /// Insert a hash with bounded FIFO eviction.
    fn insert_hash(&mut self, h: u64) -> bool {
        if !self.story_hashes.insert(h) {
            return false; // already seen
        }
        self.story_hash_order.push_back(h);
        // Evict oldest when over capacity
        while self.story_hashes.len() > MAX_STORY_HASHES {
            if let Some(old) = self.story_hash_order.pop_front() {
                self.story_hashes.remove(&old);
            }
        }
        true
    }

    /// Filter lines through noise detection and deduplication.
    ///
    /// Each line is paired with a `bool` indicating whether it came from a
    /// behaviorally noisy row. Lines that pass both noise checks are then
    /// hash-deduplicated against the story.
    fn filter_new_lines(&mut self, lines: Vec<(String, bool)>) -> Vec<String> {
        let mut result = Vec::new();
        for (line, behaviorally_noisy) in lines {
            // Layer 1: behavioral — row was rewritten many times
            if behaviorally_noisy {
                continue;
            }
            // Layer 2: structural fallback — one-shot noise patterns
            if NoiseClassifier::is_noise(&line) {
                continue;
            }
            // Hash dedup against the story
            let h = Self::hash_line(&line);
            if self.insert_hash(h) {
                result.push(line);
            }
        }
        result
    }
}

impl Transform for TerminalTransform {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output_events = Vec::with_capacity(events.len());
        let mut accumulated_time = 0.0;

        for event in events.drain(..) {
            match event.event_type {
                EventType::Output => {
                    let mut scrolled_lines = Vec::new();
                    {
                        let mut scroll_cb = |cells: Vec<crate::terminal::Cell>| {
                            let line: String = cells.iter().map(|c| c.char).collect::<String>();
                            scrolled_lines.push(line.trim_end().to_string());
                        };
                        self.buffer.process(&event.data, Some(&mut scroll_cb));
                    }
                    accumulated_time += event.time;

                    // Track which row the cursor landed on after processing
                    let cursor_row = self.buffer.cursor_row();
                    if cursor_row < self.row_write_counts.len() {
                        self.row_write_counts[cursor_row] += 1;
                    }

                    // 1. Emit lines that were scrolled off the screen immediately
                    let had_scroll = !scrolled_lines.is_empty();
                    let scroll_count = scrolled_lines.len();
                    if had_scroll {
                        // Tag each scrolled line with its row's noise status
                        // before shifting counts. Scrolled lines come from the
                        // top rows (0..scroll_count).
                        let tagged: Vec<(String, bool)> = scrolled_lines
                            .into_iter()
                            .enumerate()
                            .map(|(i, line)| {
                                let noisy = self.is_noisy_row(i);
                                (line, noisy)
                            })
                            .collect();

                        // Shift row counts now that those rows are gone
                        self.shift_row_counts(scroll_count);

                        let new_lines = self.filter_new_lines(tagged);
                        if !new_lines.is_empty() {
                            output_events
                                .push(Event::output(accumulated_time, new_lines.join("\n")));
                            accumulated_time = 0.0;
                        }
                    }

                    let current_cursor = (self.buffer.cursor_row(), self.buffer.cursor_col());

                    // Optimization: only snapshot the buffer when something
                    // interesting happened (cursor moved, scroll, newline, or
                    // long pause). Skipping to_string() for typing-within-line
                    // events eliminates the dominant cost on large files.
                    let cursor_moved = current_cursor != self.last_cursor_pos;
                    let has_newline = event.data.contains('\n');
                    let long_pause = event.time > 2.0;

                    if cursor_moved || had_scroll || has_newline || long_pause {
                        let current_display = self.buffer.to_string();
                        let current_lines: Vec<String> =
                            current_display.lines().map(|s| s.to_string()).collect();

                        // Logic: lines ABOVE the cursor are considered stable and finished.
                        let mut lines_to_emit: Vec<(String, bool)> = Vec::new();

                        // 2. Identify lines that the cursor has moved past
                        while self.stable_lines_count < current_cursor.0
                            && self.stable_lines_count < current_lines.len()
                        {
                            let row = self.stable_lines_count;
                            let noisy = self.is_noisy_row(row);
                            lines_to_emit.push((current_lines[row].clone(), noisy));
                            self.stable_lines_count += 1;
                        }

                        // 3. Emit the current line IF it was finalized
                        let is_stable =
                            has_newline || current_cursor.0 < self.last_cursor_pos.0 || long_pause;

                        if is_stable
                            && current_cursor.0 < current_lines.len()
                            && self.stable_lines_count <= current_cursor.0
                        {
                            let row = current_cursor.0;
                            let noisy = self.is_noisy_row(row);
                            lines_to_emit.push((current_lines[row].clone(), noisy));
                            if has_newline {
                                self.stable_lines_count = current_cursor.0 + 1;
                            }
                        }

                        if !lines_to_emit.is_empty() {
                            let new_lines = self.filter_new_lines(lines_to_emit);
                            if !new_lines.is_empty() {
                                output_events
                                    .push(Event::output(accumulated_time, new_lines.join("\n")));
                                accumulated_time = 0.0;
                            }
                        }
                    }

                    self.last_cursor_pos = current_cursor;
                }
                EventType::Resize => {
                    if let Some((w, h)) = event.parse_resize() {
                        self.buffer.resize(w as usize, h as usize);
                        self.row_write_counts.resize(h as usize, 0);
                        let mut e = event;
                        e.time += accumulated_time;
                        accumulated_time = 0.0;
                        output_events.push(e);
                    }
                }
                _ => {
                    let mut e = event;
                    e.time += accumulated_time;
                    accumulated_time = 0.0;
                    output_events.push(e);
                }
            }
        }

        // Final flush
        let current_display = self.buffer.to_string();
        let current_lines: Vec<String> = current_display
            .lines()
            .map(|s| s.trim_end().to_string())
            .collect();
        let mut final_lines: Vec<(String, bool)> = Vec::new();
        while self.stable_lines_count < current_lines.len() {
            let row = self.stable_lines_count;
            let noisy = self.is_noisy_row(row);
            final_lines.push((current_lines[row].clone(), noisy));
            self.stable_lines_count += 1;
        }
        if let Some(text) = {
            let filtered = self.filter_new_lines(final_lines);
            if filtered.is_empty() {
                None
            } else {
                Some(filtered.join("\n"))
            }
        } {
            output_events.push(Event::output(accumulated_time, text));
        }

        *events = output_events;
    }
}
