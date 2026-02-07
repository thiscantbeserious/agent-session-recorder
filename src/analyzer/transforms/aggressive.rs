//! Aggressive semantic noise reduction transforms.
//!
//! These transforms perform deeper analysis of the content to remove
//! highly redundant or excessively large data blocks that are not
//! useful for LLM analysis.

use crate::asciicast::{Event, Transform};
use std::collections::{HashMap, HashSet, VecDeque};

/// Collapses consecutive lines that are highly similar.
///
/// Uses a Jaccard-based similarity threshold to identify redundant log lines
/// that vary slightly (e.g. timestamps or IDs).
pub struct SimilarityFilter {
    threshold: f64,
    last_line: Option<String>,
    skip_count: usize,
    total_collapsed: usize,
}

impl SimilarityFilter {
    /// Create a new similarity filter with the given threshold (0.0 to 1.0).
    pub fn new(threshold: f64) -> Self {
        Self {
            threshold,
            last_line: None,
            skip_count: 0,
            total_collapsed: 0,
        }
    }

    /// Calculate a shift-resistant similarity score between two strings.
    /// Uses a prefix-weighted approach to prevent collapsing different commands.
    pub fn calculate_similarity(s1: &str, s2: &str) -> f64 {
        if s1 == s2 {
            return 1.0;
        }
        if s1.is_empty() || s2.is_empty() {
            return 0.0;
        }

        let len1 = s1.chars().count();
        let len2 = s2.chars().count();

        // Don't even try to collapse short lines (likely commands or important labels)
        if len1 < 30 || len2 < 30 {
            return 0.0;
        }

        // Check for shared prefix length
        let shared_prefix = s1
            .chars()
            .zip(s2.chars())
            .take_while(|(c1, c2)| c1 == c2)
            .count();

        let prefix_ratio = shared_prefix as f64 / len1.max(len2) as f64;

        // If they share a significant prefix (e.g. same log source),
        // then check character distribution
        if prefix_ratio > 0.4 {
            let set1: HashSet<char> = s1.chars().collect();
            let set2: HashSet<char> = s2.chars().collect();
            let intersection = set1.intersection(&set2).count();
            let union = set1.union(&set2).count();
            let jaccard = intersection as f64 / union as f64;

            (prefix_ratio * 0.7) + (jaccard * 0.3)
        } else {
            0.0
        }
    }

    fn flush_skips(&mut self, accumulated_time: f64) -> Option<Event> {
        if self.skip_count > 0 {
            let msg = format!("\n[... collapsed {} similar lines ...]\n", self.skip_count);
            self.total_collapsed += self.skip_count;
            self.skip_count = 0;
            // Always return time with the skip message to preserve duration
            Some(Event::output(accumulated_time, msg))
        } else {
            None
        }
    }

    /// Get the total number of lines collapsed by this filter.
    pub fn collapsed_count(&self) -> usize {
        self.total_collapsed
    }
}

impl Transform for SimilarityFilter {
    /// Process events and collapse similar consecutive lines.
    /// Preserves cumulative time by adding deltas to the next kept event.
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output_events = Vec::with_capacity(events.len());
        let mut accumulated_time = 0.0;

        for mut event in events.drain(..) {
            if !event.is_output() {
                if let Some(skip_event) = self.flush_skips(accumulated_time) {
                    output_events.push(skip_event);
                    accumulated_time = 0.0;
                }
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output_events.push(event);
                continue;
            }

            let mut new_data = String::with_capacity(event.data.len());
            for line in event.data.split_inclusive('\n') {
                let trimmed_line = line.trim();

                let similarity = if let Some(ref last) = self.last_line {
                    Self::calculate_similarity(last, trimmed_line)
                } else {
                    0.0
                };

                if similarity >= self.threshold {
                    self.skip_count += 1;
                } else {
                    if let Some(skip_event) = self.flush_skips(accumulated_time) {
                        new_data.push_str(&skip_event.data);
                        accumulated_time = 0.0;
                    }
                    new_data.push_str(line);
                    // Only track as last_line if it was substantial
                    if trimmed_line.len() >= 30 {
                        self.last_line = Some(trimmed_line.to_string());
                    } else {
                        self.last_line = None;
                    }
                }
            }

            if !new_data.is_empty() {
                event.data = new_data;
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output_events.push(event);
            } else {
                accumulated_time += event.time;
            }
        }

        if let Some(skip_event) = self.flush_skips(accumulated_time) {
            output_events.push(skip_event);
        } else if accumulated_time > 0.0 {
            if let Some(last) = output_events.last_mut() {
                last.time += accumulated_time;
            }
        }

        *events = output_events;
    }
}

/// Truncates large contiguous blocks of output.
///
/// Preserves head and tail context while removing the middle of massive
/// output events (e.g. large file dumps).
pub struct BlockTruncator {
    max_size: usize,
    context_lines: usize,
    total_truncated: usize,
}

impl BlockTruncator {
    /// Create a new truncator with the given size limit and context lines.
    pub fn new(max_size: usize, context_lines: usize) -> Self {
        Self {
            max_size,
            context_lines,
            total_truncated: 0,
        }
    }

    /// Get the total number of blocks truncated.
    pub fn truncated_count(&self) -> usize {
        self.total_truncated
    }

    fn truncate(&mut self, data: &str) -> String {
        if data.len() <= self.max_size {
            return data.to_string();
        }
        self.total_truncated += 1;
        let lines: Vec<&str> = data.split_inclusive('\n').collect();
        if lines.len() <= self.context_lines * 2 {
            let head_len = self.max_size / 2;
            let head: String = data.chars().take(head_len).collect();
            let tail: String = data
                .chars()
                .rev()
                .take(head_len)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            return format!(
                "{}\n\n[... truncated {} bytes ...]\n\n{}",
                head,
                data.len() - (head.len() + tail.len()),
                tail
            );
        }
        let head: String = lines[..self.context_lines].concat();
        let tail: String = lines[lines.len() - self.context_lines..].concat();
        format!(
            "{}\n[... truncated {} lines ...]\n{}",
            head,
            lines.len() - (self.context_lines * 2),
            tail
        )
    }
}

impl Transform for BlockTruncator {
    /// Truncates individual output events that exceed size limits.
    fn transform(&mut self, events: &mut Vec<Event>) {
        for event in events.iter_mut() {
            if event.is_output() {
                event.data = self.truncate(&event.data);
            }
        }
    }
}

/// Coalesces consecutive output events that are extremely similar.
///
/// Targets rapid TUI redrawing where multiple small events represent
/// the same visual state updated at high frequency.
pub struct EventCoalescer {
    threshold: f64,
    time_threshold: f64,
    last_event: Option<Event>,
    coalesced_count: usize,
}

impl EventCoalescer {
    /// Create a new coalescer with similarity and time thresholds.
    pub fn new(threshold: f64, time_threshold: f64) -> Self {
        Self {
            threshold,
            time_threshold,
            last_event: None,
            coalesced_count: 0,
        }
    }

    /// Get the total number of events merged.
    pub fn coalesced_count(&self) -> usize {
        self.coalesced_count
    }
}

impl Transform for EventCoalescer {
    /// Merges rapid, similar consecutive events into one.
    /// Sums time deltas to preserve session duration.
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output_events = Vec::with_capacity(events.len());
        for event in events.drain(..) {
            if !event.is_output() {
                if let Some(le) = self.last_event.take() {
                    output_events.push(le);
                }
                output_events.push(event);
                continue;
            }
            if let Some(mut le) = self.last_event.take() {
                let similarity = SimilarityFilter::calculate_similarity(&le.data, &event.data);
                if similarity >= self.threshold && event.time <= self.time_threshold {
                    self.coalesced_count += 1;
                    le.data = event.data;
                    le.time += event.time;
                    self.last_event = Some(le);
                } else {
                    output_events.push(le);
                    self.last_event = Some(event);
                }
            } else {
                self.last_event = Some(event);
            }
        }
        if let Some(le) = self.last_event.take() {
            output_events.push(le);
        }
        *events = output_events;
    }
}

/// Global deduplication of repetitive lines and windowed event hashing.
///
/// Implements a global frequency cap for lines and a sliding window for
/// exact event content hashing to catch redundant TUI redraws.
///
/// **Important**: Windowed hashing only applies to events larger than
/// `min_hash_bytes` to avoid discarding small but meaningful events
/// like individual keystrokes and short output fragments.
pub struct GlobalDeduplicator {
    line_counts: HashMap<String, usize>,
    max_line_repeats: usize,
    event_hashes: VecDeque<u64>,
    event_hash_set: HashSet<u64>,
    window_size: usize,
    min_hash_bytes: usize,
    total_deduped_lines: usize,
    total_deduped_events: usize,
}

/// Minimum event data size (bytes) for windowed hash deduplication.
/// Events smaller than this are kept regardless of duplicates, since they
/// typically represent keystrokes or short output rather than TUI redraws.
const DEFAULT_MIN_HASH_BYTES: usize = 128;

impl GlobalDeduplicator {
    /// Create a new global deduplicator.
    pub fn new(max_line_repeats: usize, window_size: usize) -> Self {
        Self {
            line_counts: HashMap::new(),
            max_line_repeats,
            event_hashes: VecDeque::with_capacity(window_size),
            event_hash_set: HashSet::with_capacity(window_size),
            window_size,
            min_hash_bytes: DEFAULT_MIN_HASH_BYTES,
            total_deduped_lines: 0,
            total_deduped_events: 0,
        }
    }

    /// Get stats: (lines_deduped, events_deduped).
    pub fn stats(&self) -> (usize, usize) {
        (self.total_deduped_lines, self.total_deduped_events)
    }

    fn hash_string(s: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }
}

impl Transform for GlobalDeduplicator {
    /// Removes redundant events and repetitive lines across the entire session.
    /// Carefully accumulates time deltas to maintain timestamp integrity.
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output_events = Vec::with_capacity(events.len());
        let mut accumulated_time = 0.0;

        for mut event in events.drain(..) {
            if !event.is_output() {
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output_events.push(event);
                continue;
            }

            // Windowed event hashing (targets TUI redraw frames)
            // Skip small events: keystrokes and short output are not redraws
            if event.data.len() >= self.min_hash_bytes {
                let h = Self::hash_string(&event.data);
                if self.event_hash_set.contains(&h) {
                    self.total_deduped_events += 1;
                    accumulated_time += event.time;
                    continue;
                }
                self.event_hashes.push_back(h);
                self.event_hash_set.insert(h);
                if self.event_hashes.len() > self.window_size {
                    if let Some(old) = self.event_hashes.pop_front() {
                        self.event_hash_set.remove(&old);
                    }
                }
            }

            // Line frequency capping (Global)
            // We keep only the last few instances of any given non-empty line
            let mut new_data = String::with_capacity(event.data.len());
            let lines: Vec<String> = event
                .data
                .split_inclusive('\n')
                .map(|s| s.to_string())
                .collect();

            for line in lines {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    new_data.push_str(&line);
                    continue;
                }

                let count = self.line_counts.entry(trimmed.to_string()).or_insert(0);
                if *count >= self.max_line_repeats {
                    self.total_deduped_lines += 1;
                    continue;
                }
                *count += 1;
                new_data.push_str(&line);
            }

            if !new_data.is_empty() {
                event.data = new_data;
                event.time += accumulated_time;
                accumulated_time = 0.0;
                output_events.push(event);
            } else {
                accumulated_time += event.time;
            }
        }

        if accumulated_time > 0.0 {
            if let Some(last) = output_events.last_mut() {
                last.time += accumulated_time;
            }
        }

        *events = output_events;
    }
}

/// Detects and collapses long bursts of output across multiple events.
///
/// This targets file dumps (e.g. cat, find) or massive log output that
/// happens in a short time window without user interaction.
pub struct FileDumpFilter {
    max_burst_lines: usize,
    burst_events: Vec<Event>,
    burst_line_count: usize,
    total_collapsed: usize,
}

impl FileDumpFilter {
    /// Create a new file dump filter with given line limit.
    pub fn new(max_burst_lines: usize) -> Self {
        Self {
            max_burst_lines,
            burst_events: Vec::new(),
            burst_line_count: 0,
            total_collapsed: 0,
        }
    }

    pub fn collapsed_count(&self) -> usize {
        self.total_collapsed
    }

    fn flush_burst(&mut self, output: &mut Vec<Event>) {
        if self.burst_events.is_empty() {
            return;
        }

        if self.burst_line_count > self.max_burst_lines {
            // Collapse the burst
            let head_count = 50; // Keep first 50 lines
            let tail_count = 10; // Keep last 10 lines

            let all_content: String = self.burst_events.iter().map(|e| e.data.as_str()).collect();
            let all_lines: Vec<&str> = all_content.split_inclusive('\n').collect();

            if all_lines.len() > (head_count + tail_count) {
                let head: String = all_lines[..head_count].concat();
                let tail: String = all_lines[all_lines.len() - tail_count..].concat();
                let collapsed = all_lines.len() - (head_count + tail_count);

                self.total_collapsed += collapsed;
                let total_time: f64 = self.burst_events.iter().map(|e| e.time).sum();

                output.push(Event::output(
                    total_time,
                    format!(
                        "{}\n[... collapsed {} lines of file/log output ...]\n{}",
                        head.trim_end(),
                        collapsed,
                        tail.trim_start()
                    ),
                ));
            } else {
                output.append(&mut self.burst_events);
            }
        } else {
            output.append(&mut self.burst_events);
        }

        self.burst_events.clear();
        self.burst_line_count = 0;
    }
}

impl Default for FileDumpFilter {
    fn default() -> Self {
        Self::new(500)
    }
}

impl Transform for FileDumpFilter {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output = Vec::with_capacity(events.len());

        for event in events.drain(..) {
            if !event.is_output() || event.time > 0.5 {
                // Non-output or significant time gap ends a burst
                self.flush_burst(&mut output);
                output.push(event);
                continue;
            }

            // Group output event into current burst
            let lines = event.data.chars().filter(|&c| c == '\n').count();
            self.burst_line_count += lines;
            self.burst_events.push(event);

            // If burst already too large, we can stop early
            if self.burst_line_count > self.max_burst_lines * 2 {
                self.flush_burst(&mut output);
            }
        }

        self.flush_burst(&mut output);
        *events = output;
    }
}

/// Deduplicates lines within a sliding window of events.
///
/// Keeps ONLY the last instance of any non-empty line that repeats
/// within the window. This is highly effective at cleaning up status
/// lines, repetitive TUI elements, and log bursts while keeping the
/// final (most relevant) state.
pub struct WindowedLineDeduplicator {
    window_size: usize,
    line_buffer: VecDeque<(String, f64)>,
    total_deduped: usize,
}

impl WindowedLineDeduplicator {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            line_buffer: VecDeque::with_capacity(window_size),
            total_deduped: 0,
        }
    }

    pub fn deduped_count(&self) -> usize {
        self.total_deduped
    }

    fn flush_lines(&mut self, output: &mut Vec<Event>) {
        if self.line_buffer.is_empty() {
            return;
        }

        let lines: Vec<_> = self.line_buffer.drain(..).collect();

        // Build a map of trimmed_end -> last index for O(1) repeat detection
        let mut last_occurrence: HashMap<&str, usize> = HashMap::with_capacity(lines.len());
        for (i, (ref line, _)) in lines.iter().enumerate() {
            let trimmed = line.trim_end();
            if !trimmed.trim().is_empty() {
                last_occurrence.insert(trimmed, i);
            }
        }

        let mut current_data = String::new();
        let mut current_time = 0.0;

        for (i, (ref line_text, time)) in lines.iter().enumerate() {
            let trimmed = line_text.trim();

            if trimmed.is_empty() {
                current_data.push_str(line_text);
                current_time += *time;
                continue;
            }

            let line_trimmed_end = line_text.trim_end();

            // O(1) repeat detection via HashMap
            let is_repeated = last_occurrence
                .get(line_trimmed_end)
                .map(|&last| last > i)
                .unwrap_or(false);

            // O(n) prefix detection only when line is not already a repeat
            let is_prefix = if !is_repeated {
                lines[(i + 1)..].iter().any(|(later, _)| {
                    let later_trimmed = later.trim_end();
                    later_trimmed.starts_with(line_trimmed_end)
                        && later_trimmed.len() > line_trimmed_end.len()
                })
            } else {
                false
            };

            if !is_prefix && !is_repeated {
                current_data.push_str(line_text);
                current_time += *time;
            } else {
                self.total_deduped += 1;
                current_time += *time;
            }
        }

        if !current_data.is_empty() {
            output.push(Event::output(current_time, current_data));
        }
    }
}

impl Default for WindowedLineDeduplicator {
    fn default() -> Self {
        Self::new(1000) // 1000-line window
    }
}

impl Transform for WindowedLineDeduplicator {
    fn transform(&mut self, events: &mut Vec<Event>) {
        let mut output = Vec::with_capacity(events.len());

        for event in events.drain(..) {
            if !event.is_output() {
                self.flush_lines(&mut output);
                output.push(event);
                continue;
            }

            let lines: Vec<String> = event
                .data
                .split_inclusive('\n')
                .map(|s| s.to_string())
                .collect();
            let line_count = lines.len();
            let time_per_line = if line_count == 0 {
                event.time
            } else {
                event.time / line_count as f64
            };

            for line in lines {
                self.line_buffer.push_back((line, time_per_line));
                if self.line_buffer.len() >= self.window_size {
                    self.flush_lines(&mut output);
                }
            }
        }

        self.flush_lines(&mut output);
        *events = output;
    }
}
