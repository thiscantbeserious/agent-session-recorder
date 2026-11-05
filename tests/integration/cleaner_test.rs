//! Moved from the inline tests of `analyzer/transforms/cleaner.rs` (public API only).

use agr::analyzer::{ContentCleaner, ExtractionConfig};

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

// Character stripping via clean() Tests
// ============================================

#[test]
fn strip_nothing_when_all_flags_false() {
    let config = ExtractionConfig {
        strip_box_drawing: false,
        strip_spinner_chars: false,
        strip_progress_blocks: false,
        ..Default::default()
    };
    let mut cleaner = ContentCleaner::new(&config);

    // Box drawing, spinner, and progress chars should all pass through
    let output = cleaner.clean("\u{2500}\u{273B}\u{2588}text");
    assert_eq!(output, "\u{2500}\u{273B}\u{2588}text");
}

#[test]
fn strip_only_box_when_flag_set() {
    let config = ExtractionConfig {
        strip_box_drawing: true,
        strip_spinner_chars: false,
        strip_progress_blocks: false,
        ..Default::default()
    };
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
    let config = ExtractionConfig {
        strip_box_drawing: false,
        strip_spinner_chars: true,
        strip_progress_blocks: false,
        ..Default::default()
    };
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
    let config = ExtractionConfig {
        strip_box_drawing: false,
        strip_spinner_chars: false,
        strip_progress_blocks: true,
        ..Default::default()
    };
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
