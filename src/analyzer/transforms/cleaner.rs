//! Single-pass content cleaner for ANSI and control character stripping.
//!
//! Uses a state machine to efficiently process content in a single pass,
//! handling ANSI escape sequences (CSI, OSC), control characters, and
//! visual-only Unicode characters.

use std::collections::HashSet;

use crate::asciicast::{Event, Transform};

use super::super::config::ExtractionConfig;

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
                    self.process_normal_char(c);
                }
            }
        }

        // Handle incomplete sequences (reset state for next event)
        if !matches!(self.ansi_state, AnsiParseState::Normal) {
            self.ansi_state = AnsiParseState::Normal;
        }

        self.buffer.clone()
    }

    /// Process a normal (non-escape) character.
    fn process_normal_char(&mut self, c: char) {
        // Check for control characters (except \t, \n, \r which have meaning)
        if c < '\x20' && c != '\t' && c != '\n' && c != '\r' {
            self.control_stripped += 1;
            return;
        }
        // DEL character
        if c == '\x7f' {
            self.control_stripped += 1;
            return;
        }
        // C1 control characters (0x80-0x9F)
        if ('\u{0080}'..='\u{009F}').contains(&c) {
            self.control_stripped += 1;
            return;
        }

        // Semantic chars are always kept
        if self.semantic_chars.contains(&c) {
            self.buffer.push(c);
            return;
        }

        // Strip configured characters
        if self.strip_chars.contains(&c) {
            return;
        }

        // Keep everything else
        self.buffer.push(c);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_csi_color_codes() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "\x1b[38;5;174mcolored\x1b[0m text";
        let output = cleaner.clean(input);
        assert_eq!(output, "colored text");
    }

    #[test]
    fn strips_cursor_movement() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "\x1b[2K\x1b[1A\x1b[Ghello";
        let output = cleaner.clean(input);
        assert_eq!(output, "hello");
    }

    #[test]
    fn strips_osc_sequences() {
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
    fn strips_control_chars() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // BEL, NUL, and other control chars should be stripped
        let input = "hello\x07\x00world";
        let output = cleaner.clean(input);
        assert_eq!(output, "helloworld");
    }

    #[test]
    fn preserves_tab_newline_cr() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "hello\tworld\nline2\roverwrite";
        let output = cleaner.clean(input);
        assert_eq!(output, "hello\tworld\nline2\roverwrite");
    }

    #[test]
    fn preserves_semantic_chars() {
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
    fn strips_box_drawing() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "╭───────╮\n│ hello │\n╰───────╯";
        let output = cleaner.clean(input);
        assert_eq!(output, "\n hello \n");
    }

    #[test]
    fn strips_claude_spinners() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "✻ Thinking... ✳ Working... ✶ Done";
        let output = cleaner.clean(input);
        assert_eq!(output, " Thinking...  Working...  Done");
    }

    #[test]
    fn strips_gemini_braille_spinners() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "⠋ Loading ⠙ Loading ⠹ Loading";
        let output = cleaner.clean(input);
        assert_eq!(output, " Loading  Loading  Loading");
    }

    #[test]
    fn strips_progress_blocks() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        let input = "Progress: ████░░░░ 50%";
        let output = cleaner.clean(input);
        assert_eq!(output, "Progress:  50%");
    }

    #[test]
    fn handles_nested_sequences() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // Color inside cursor movement
        let input = "\x1b[2K\x1b[38;5;174mtext\x1b[0m\x1b[1G";
        let output = cleaner.clean(input);
        assert_eq!(output, "text");
    }

    #[test]
    fn handles_partial_sequences() {
        let config = ExtractionConfig::default();
        let mut cleaner = ContentCleaner::new(&config);

        // Incomplete CSI at end
        let input = "hello\x1b[";
        let output = cleaner.clean(input);
        assert_eq!(output, "hello");
    }
}
