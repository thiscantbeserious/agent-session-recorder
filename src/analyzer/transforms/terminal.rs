//! Terminal emulation transform.
//!
//! Uses a virtual terminal buffer to process events, handling ANSI escape
//! sequences and carriage return overwrites correctly. This produces a
//! "rendered" version of the terminal state, which is much cleaner for
//! TUI sessions and preserves spatial layout (indentation).

use crate::asciicast::{Event, EventType, Transform};
use crate::terminal::TerminalBuffer;

/// A transform that renders events through a virtual terminal and extracts
/// stable lines to build a clean chronological log.
pub struct TerminalTransform {
    buffer: TerminalBuffer,
    /// Number of stable lines already emitted
    stable_lines_count: usize,
    /// Last cursor row to detect when a line is "passed over"
    last_cursor_pos: (usize, usize),
}

impl TerminalTransform {
    /// Create a new terminal transform with given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            buffer: TerminalBuffer::new(width, height),
            stable_lines_count: 0,
            last_cursor_pos: (0, 0),
        }
    }

    /// Check if a line is "razzle dazzle" thinking noise or status bar.
    fn is_noise(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        
        // Target specific TUI status patterns
        trimmed.contains("Shimmying…") || 
        trimmed.contains("Orbiting…") || 
        trimmed.contains("Improvising…") || 
        trimmed.contains("Whatchamacalliting…") ||
        trimmed.contains("Churning…") ||
        trimmed.contains("Clauding…") ||
        trimmed.contains("Razzle-dazzling…") ||
        trimmed.contains("Wibbling…") ||
        trimmed.contains("Bloviating…") ||
        trimmed.contains("Herding…") ||
        trimmed.contains("Channeling…") ||
        trimmed.contains("Unfurling…") ||
        trimmed.contains("accept edits on (shift+Tab to cycle)") ||
        trimmed.contains("Context left until auto-compact") ||
        trimmed.contains("thinking") ||
        trimmed.contains("Tip:") ||
        trimmed.contains("Update available!") ||
        (trimmed.contains("Done") && trimmed.contains("tool uses"))
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
                            let line: String = cells.iter().map(|c| c.char).collect();
                            scrolled_lines.push(line.trim_end().to_string());
                        };
                        self.buffer.process(&event.data, Some(&mut scroll_cb));
                    }
                    accumulated_time += event.time;

                    // 1. Emit lines that were scrolled off the screen immediately
                    if !scrolled_lines.is_empty() {
                        let filtered: Vec<_> = scrolled_lines.into_iter().filter(|l| !Self::is_noise(l)).collect();
                        if !filtered.is_empty() {
                            output_events.push(Event::output(accumulated_time, filtered.join("\n")));
                            accumulated_time = 0.0;
                        }
                    }

                    let current_cursor = (self.buffer.cursor_row(), self.buffer.cursor_col());
                    let current_display = self.buffer.to_string();
                    let current_lines: Vec<String> = current_display
                        .lines()
                        .map(|s| s.trim_end().to_string())
                        .collect();

                    // Logic: lines ABOVE the cursor are considered stable and finished.
                    let mut lines_to_emit = Vec::new();
                    
                    // 2. Emit lines that the cursor has moved past
                    while self.stable_lines_count < current_cursor.0 && self.stable_lines_count < current_lines.len() {
                        let line = &current_lines[self.stable_lines_count];
                        if !Self::is_noise(line) {
                            lines_to_emit.push(line.clone());
                        }
                        self.stable_lines_count += 1;
                    }

                    // 3. Emit the current line IF it was finalized (newline) or we hit a long pause
                    // or if the cursor moved BACKWARD/UP (command finished or clear)
                    let is_typing_in_progress = current_cursor.0 == self.last_cursor_pos.0 && 
                                              current_cursor.1 > self.last_cursor_pos.1;
                    
                    let is_stable = event.data.contains('\n') || 
                                   (!is_typing_in_progress && (current_cursor.0 < self.last_cursor_pos.0 || current_cursor.1 < self.last_cursor_pos.1)) || 
                                   event.time > 2.0;

                    if is_stable && current_cursor.0 < current_lines.len() {
                        if self.stable_lines_count <= current_cursor.0 {
                            let line = &current_lines[current_cursor.0];
                            if !Self::is_noise(line) {
                                lines_to_emit.push(line.clone());
                            }
                            if event.data.contains('\n') {
                                self.stable_lines_count = current_cursor.0 + 1;
                            }
                        }
                    }

                    if !lines_to_emit.is_empty() {
                        output_events.push(Event::output(accumulated_time, lines_to_emit.join("\n")));
                        accumulated_time = 0.0;
                    }
                    
                    self.last_cursor_pos = current_cursor;
                }
                EventType::Resize => {
                    if let Some((w, h)) = event.parse_resize() {
                        self.buffer.resize(w as usize, h as usize);
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
        let current_lines: Vec<String> = current_display.lines().map(|s| s.trim_end().to_string()).collect();
        let mut final_lines = Vec::new();
        while self.stable_lines_count < current_lines.len() {
            let line = &current_lines[self.stable_lines_count];
            if !Self::is_noise(line) {
                final_lines.push(line.clone());
            }
            self.stable_lines_count += 1;
        }
        if !final_lines.is_empty() {
            output_events.push(Event::output(accumulated_time, final_lines.join("\n")));
        }

        *events = output_events;
    }
}