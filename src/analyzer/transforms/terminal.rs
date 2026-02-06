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
    /// The last full rendered frame content (trimmed) to detect changes
    last_frame_content: Vec<String>,
    /// Last cursor position to detect and skip typing increments
    last_cursor_pos: (usize, usize),
}

impl TerminalTransform {
    /// Create a new terminal transform with given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            buffer: TerminalBuffer::new(width, height),
            last_frame_content: Vec::new(),
            last_cursor_pos: (0, 0),
        }
    }

    /// Check if a line is "razzle dazzle" thinking noise.
    fn is_noise(line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return false;
        }
        
        trimmed.contains("Shimmying…") || 
        trimmed.contains("Orbiting…") || 
        trimmed.contains("Improvising…") || 
        trimmed.contains("Whatchamacalliting…") ||
        trimmed.contains("Churning…") ||
        trimmed.contains("Clauding…") ||
        trimmed.contains("Razzle-dazzling…") ||
        trimmed.contains("Wibbling…") ||
        trimmed.contains("accept edits on (shift+Tab to cycle)") ||
        trimmed.contains("Context left until auto-compact") ||
        trimmed.contains("thinking") ||
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
                    self.buffer.process(&event.data);
                    accumulated_time += event.time;

                    // We only emit a frame when it's "stable".
                    // Stable means:
                    // 1. A newline was just processed (line finalized)
                    // 2. The cursor moved BACKWARD or UP (indicates return, clear, or scroll)
                    // 3. A significant pause occurred (user stopped typing or agent is thinking)
                    let current_cursor = (self.buffer.cursor_row(), self.buffer.cursor_col());
                    let is_typing_in_progress = current_cursor.0 == self.last_cursor_pos.0 && 
                                              current_cursor.1 > self.last_cursor_pos.1;
                    
                    let is_stable = event.data.contains('\n') || 
                                   (!is_typing_in_progress && (current_cursor.0 < self.last_cursor_pos.0 || current_cursor.1 < self.last_cursor_pos.1)) || 
                                   event.time > 2.0;

                    if is_stable {
                        let current_display = self.buffer.to_string();
                        let current_lines: Vec<String> = current_display
                            .lines()
                            .map(|s| s.trim_end().to_string()) // Trim trailing spaces for comparison/token reduction
                            .collect();

                        // Identify new stable lines that appeared since last emission.
                        let mut lines_to_emit = Vec::new();
                        for line in &current_lines {
                            if !self.last_frame_content.contains(line) && !Self::is_noise(line) {
                                lines_to_emit.push(line.clone());
                            }
                        }

                        if !lines_to_emit.is_empty() {
                            output_events.push(Event::output(accumulated_time, lines_to_emit.join("\n")));
                            accumulated_time = 0.0;
                        }
                        
                        self.last_frame_content = current_lines;
                        self.last_cursor_pos = current_cursor;
                    }
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

        *events = output_events;
    }
}