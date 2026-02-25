//! Single-pass content cleaner for ANSI and control character stripping.
//!
//! Uses a state machine to efficiently process content in a single pass,
//! handling ANSI escape sequences (CSI, OSC), control characters, and
//! visual-only Unicode characters.

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
    /// Strip box drawing (U+2500–U+257F) and block elements (U+2580–U+259F)
    strip_box_drawing: bool,
    /// Strip spinner animation characters
    strip_spinner_chars: bool,
    /// Strip progress bar block characters
    strip_progress_blocks: bool,
    /// Statistics tracking
    ansi_stripped: usize,
    control_stripped: usize,
}

/// Returns true for characters with semantic meaning that must never be stripped.
#[inline]
const fn is_semantic_char(c: char) -> bool {
    matches!(
        c,
        '\u{2713}' // ✓ Check mark
        | '\u{2714}' // ✔ Heavy check mark
        | '\u{2715}' // ✕ Multiplication X
        | '\u{26A0}' // ⚠ Warning sign
        | '\u{2139}' // ℹ Information source
        | '\u{2610}' // ☐ Ballot box
        | '\u{2611}' // ☑ Ballot box with check
    )
}

/// Box drawing characters (U+2500–U+257F) and block elements (U+2580–U+259F).
#[inline]
const fn is_box_drawing(c: char) -> bool {
    matches!(c, '\u{2500}'..='\u{257F}' | '\u{2580}'..='\u{259F}')
}

/// Spinner animation characters (visual-only, no semantic meaning).
#[inline]
const fn is_spinner_char(c: char) -> bool {
    matches!(
        c,
        // Claude spinners
        '\u{273B}'   // ✻
        | '\u{2733}' // ✳
        | '\u{2722}' // ✢
        | '\u{2736}' // ✶
        | '\u{273D}' // ✽
        // Gemini braille spinner frames
        | '\u{280B}' // ⠋
        | '\u{2819}' // ⠙
        | '\u{2839}' // ⠹
        | '\u{2838}' // ⠸
        | '\u{283C}' // ⠼
        | '\u{2834}' // ⠴
        | '\u{2826}' // ⠦
        | '\u{2827}' // ⠧
        | '\u{2807}' // ⠇
        | '\u{280F}' // ⠏
        // Visual-only bullets and list markers
        | '\u{2022}' // •
        | '\u{203A}' // ›
        | '\u{25E6}' // ◦
        | '\u{22EE}' // ⋮
    )
}

/// Progress bar block and indicator characters.
#[inline]
const fn is_progress_block(c: char) -> bool {
    matches!(
        c,
        // Block fill characters
        '\u{2588}'   // █ Full block
        | '\u{2591}' // ░ Light shade
        | '\u{2592}' // ▒ Medium shade
        | '\u{2593}' // ▓ Dark shade
        // Progress indicators
        | '\u{25BC}' // ▼ Down triangle
        | '\u{25B2}' // ▲ Up triangle
        | '\u{25CF}' // ● Filled circle
        | '\u{25CB}' // ○ Empty circle
    )
}

impl ContentCleaner {
    /// Create a new content cleaner with the given configuration.
    pub fn new(config: &ExtractionConfig) -> Self {
        Self {
            buffer: String::with_capacity(4096),
            ansi_state: AnsiParseState::Normal,
            strip_box_drawing: config.strip_box_drawing,
            strip_spinner_chars: config.strip_spinner_chars,
            strip_progress_blocks: config.strip_progress_blocks,
            ansi_stripped: 0,
            control_stripped: 0,
        }
    }

    /// Process event data in a single pass, returns cleaned string.
    pub fn clean(&mut self, data: &str) -> String {
        self.buffer.clear();

        for c in data.chars() {
            if self.ansi_state == AnsiParseState::Normal {
                if c == '\x1b' {
                    self.ansi_state = AnsiParseState::Escape;
                    self.ansi_stripped += 1;
                } else {
                    self.process_normal_char(c);
                }
            } else {
                self.handle_escape_char(c);
            }
        }

        self.buffer.clone()
    }

    /// Advance the ANSI state machine for a character received while inside
    /// an escape sequence (any state other than Normal).
    ///
    /// All state transitions for Escape, Csi, CsiParams, Osc, and OscEscape
    /// are handled here, keeping `clean()` free of their detail.
    fn handle_escape_char(&mut self, c: char) {
        match self.ansi_state {
            AnsiParseState::Escape => {
                match c {
                    '[' => self.ansi_state = AnsiParseState::Csi,
                    ']' => self.ansi_state = AnsiParseState::Osc,
                    // Simple escape: ESC followed by alphabetic or charset designator
                    _ if c.is_ascii_alphabetic() || c == '(' || c == ')' => {
                        // Skip this char (e.g. ESC c, ESC D, ESC M, ESC ( / ESC ))
                        self.ansi_state = AnsiParseState::Normal;
                    }
                    // Any other char: invalid sequence, reset
                    _ => self.ansi_state = AnsiParseState::Normal,
                }
            }
            AnsiParseState::Csi | AnsiParseState::CsiParams => {
                if c.is_ascii_digit() || c == ';' || c == '?' || c == '>' || c == '!' {
                    // CSI parameter byte
                    self.ansi_state = AnsiParseState::CsiParams;
                } else if c.is_ascii_alphabetic() || c == '@' || c == '`' {
                    // CSI final byte: sequence ends
                    self.ansi_state = AnsiParseState::Normal;
                } else {
                    // Unexpected byte: reset
                    self.ansi_state = AnsiParseState::Normal;
                }
            }
            AnsiParseState::Osc => match c {
                '\x07' => self.ansi_state = AnsiParseState::Normal, // BEL terminates OSC
                '\x1b' => self.ansi_state = AnsiParseState::OscEscape, // possible ST
                _ => {}                                             // inside OSC: skip content
            },
            AnsiParseState::OscEscape => {
                if c == '\\' {
                    // ST terminator (ESC \) complete
                    self.ansi_state = AnsiParseState::Normal;
                } else {
                    // Not a valid ST: stay in OSC
                    self.ansi_state = AnsiParseState::Osc;
                }
            }
            AnsiParseState::Normal => {
                unreachable!("handle_escape_char is only called for non-Normal states")
            }
        }
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
        if is_semantic_char(c) {
            self.buffer.push(c);
            return;
        }

        // Strip configured visual-only characters
        if self.is_strip_char(c) {
            return;
        }

        // Keep everything else
        self.buffer.push(c);
    }

    /// Check if a character is a visual-only character that should be stripped
    /// according to the current config flags.
    #[inline]
    fn is_strip_char(&self, c: char) -> bool {
        (self.strip_box_drawing && is_box_drawing(c))
            || (self.strip_spinner_chars && is_spinner_char(c))
            || (self.strip_progress_blocks && is_progress_block(c))
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

    // ============================================
    // is_semantic_char Tests
    // ============================================

    #[test]
    fn is_semantic_char_all_7() {
        assert!(is_semantic_char('\u{2713}')); // ✓
        assert!(is_semantic_char('\u{2714}')); // ✔
        assert!(is_semantic_char('\u{2715}')); // ✕
        assert!(is_semantic_char('\u{26A0}')); // ⚠
        assert!(is_semantic_char('\u{2139}')); // ℹ
        assert!(is_semantic_char('\u{2610}')); // ☐
        assert!(is_semantic_char('\u{2611}')); // ☑
                                               // Non-semantic chars return false
        assert!(!is_semantic_char('A'));
        assert!(!is_semantic_char('\u{2500}')); // ─ box drawing
    }

    #[test]
    fn semantic_chars_not_in_box_range() {
        for c in '\u{2500}'..='\u{257F}' {
            assert!(
                !is_semantic_char(c),
                "Box-drawing char U+{:04X} should not be semantic",
                c as u32
            );
        }
    }

    // ============================================
    // Character stripping via clean() Tests
    // ============================================

    #[test]
    fn strip_nothing_when_all_flags_false() {
        let mut config = ExtractionConfig::default();
        config.strip_box_drawing = false;
        config.strip_spinner_chars = false;
        config.strip_progress_blocks = false;
        let mut cleaner = ContentCleaner::new(&config);

        // Box drawing, spinner, and progress chars should all pass through
        let output = cleaner.clean("\u{2500}\u{273B}\u{2588}text");
        assert_eq!(output, "\u{2500}\u{273B}\u{2588}text");
    }

    #[test]
    fn strip_only_box_when_flag_set() {
        let mut config = ExtractionConfig::default();
        config.strip_box_drawing = true;
        config.strip_spinner_chars = false;
        config.strip_progress_blocks = false;
        let mut cleaner = ContentCleaner::new(&config);

        // Box drawing stripped
        assert_eq!(cleaner.clean("\u{2500}text"), "text");
        // Block elements (U+2580-U+259F) also stripped by box_drawing flag
        assert_eq!(cleaner.clean("\u{2588}text"), "text");
        // Spinner chars preserved
        assert!(cleaner.clean("\u{273B}text").contains('\u{273B}'));
        // Progress-only chars preserved
        assert!(cleaner.clean("\u{25BC}text").contains('\u{25BC}'));
    }

    #[test]
    fn strip_only_spinner_when_flag_set() {
        let mut config = ExtractionConfig::default();
        config.strip_box_drawing = false;
        config.strip_spinner_chars = true;
        config.strip_progress_blocks = false;
        let mut cleaner = ContentCleaner::new(&config);

        // Spinner chars stripped
        assert_eq!(cleaner.clean("\u{273B}text"), "text");
        assert_eq!(cleaner.clean("\u{280B}text"), "text");
        // Box drawing preserved
        assert!(cleaner.clean("\u{2500}text").contains('\u{2500}'));
        // Block elements preserved (only stripped by box_drawing flag)
        assert!(cleaner.clean("\u{2588}text").contains('\u{2588}'));
    }

    #[test]
    fn strip_only_progress_when_flag_set() {
        let mut config = ExtractionConfig::default();
        config.strip_box_drawing = false;
        config.strip_spinner_chars = false;
        config.strip_progress_blocks = true;
        let mut cleaner = ContentCleaner::new(&config);

        // Progress blocks stripped
        assert_eq!(cleaner.clean("\u{2588}text"), "text");
        assert_eq!(cleaner.clean("\u{2591}text"), "text");
        // Spinner chars preserved
        assert!(cleaner.clean("\u{273B}text").contains('\u{273B}'));
        // Box drawing preserved
        assert!(cleaner.clean("\u{2500}text").contains('\u{2500}'));
    }

    #[test]
    fn semantic_chars_never_stripped() {
        let config = ExtractionConfig::default(); // all strip flags true
        let mut cleaner = ContentCleaner::new(&config);

        let semantic = [
            '\u{2713}', '\u{2714}', '\u{2715}', '\u{26A0}', '\u{2139}', '\u{2610}', '\u{2611}',
        ];
        for c in semantic {
            let input = format!("before{}after", c);
            let output = cleaner.clean(&input);
            assert!(
                output.contains(c),
                "Semantic char U+{:04X} was incorrectly stripped",
                c as u32
            );
        }
    }
}
