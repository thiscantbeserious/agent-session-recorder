//! Behavior-preserving integration tests for ContentCleaner.
//!
//! These tests replicate the inline tests from `src/analyzer/transforms/cleaner.rs`
//! and establish a green baseline before the Stage 1c complexity-reduction refactor.

use agr::analyzer::{ContentCleaner, ExtractionConfig};
use agr::asciicast::Transform;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a default-configured cleaner.
fn default_cleaner() -> ContentCleaner {
    ContentCleaner::new(&ExtractionConfig::default())
}

// ---------------------------------------------------------------------------
// Tests -- replicate all 10 inline tests, then add coverage for the
// edge cases called out in the PLAN.
// ---------------------------------------------------------------------------

/// CSI color codes are stripped.
/// Replicates `strips_csi_color_codes`.
#[test]
fn csi_color_codes_are_stripped() {
    let mut cleaner = default_cleaner();
    let input = "\x1b[38;5;174mcolored\x1b[0m text";
    assert_eq!(cleaner.clean(input), "colored text");
}

/// Cursor movement escape sequences are stripped.
/// Replicates `strips_cursor_movement`.
#[test]
fn cursor_movement_sequences_are_stripped() {
    let mut cleaner = default_cleaner();
    let input = "\x1b[2K\x1b[1A\x1b[Ghello";
    assert_eq!(cleaner.clean(input), "hello");
}

/// OSC sequences terminated by BEL are stripped.
/// Replicates part of `strips_osc_sequences`.
#[test]
fn osc_bel_terminated_sequence_is_stripped() {
    let mut cleaner = default_cleaner();
    let input = "\x1b]0;Window Title\x07visible";
    assert_eq!(cleaner.clean(input), "visible");
}

/// OSC sequences terminated by ST (ESC \) are stripped.
/// Replicates part of `strips_osc_sequences`.
#[test]
fn osc_st_terminated_sequence_is_stripped() {
    let mut cleaner = default_cleaner();
    let input = "\x1b]8;;http://example.com\x1b\\link\x1b]8;;\x1b\\";
    assert_eq!(cleaner.clean(input), "link");
}

/// BEL, NUL and other control chars below 0x20 (except \t, \n, \r) are stripped.
/// Replicates `strips_control_chars`.
#[test]
fn control_characters_are_stripped() {
    let mut cleaner = default_cleaner();
    let input = "hello\x07\x00world";
    assert_eq!(cleaner.clean(input), "helloworld");
}

/// Tab, newline and carriage return are always preserved.
/// Replicates `preserves_tab_newline_cr`.
#[test]
fn tab_newline_and_cr_are_preserved() {
    let mut cleaner = default_cleaner();
    let input = "hello\tworld\nline2\roverwrite";
    assert_eq!(cleaner.clean(input), "hello\tworld\nline2\roverwrite");
}

/// Semantic characters are never stripped.
/// Replicates `preserves_semantic_chars`.
#[test]
fn semantic_characters_are_preserved() {
    let mut cleaner = default_cleaner();
    let input = "test \u{2713} pass \u{2714} done \u{2715} fail \u{26A0} warn";
    let output = cleaner.clean(input);
    assert!(output.contains('\u{2713}'), "checkmark must be preserved");
    assert!(
        output.contains('\u{2714}'),
        "heavy checkmark must be preserved"
    );
    assert!(output.contains('\u{2715}'), "X mark must be preserved");
    assert!(output.contains('\u{26A0}'), "warning must be preserved");
}

/// Box drawing characters are stripped when strip_box_drawing is enabled.
/// Replicates `strips_box_drawing`.
#[test]
fn box_drawing_characters_are_stripped() {
    let mut cleaner = default_cleaner();
    let input = "╭───────╮\n│ hello │\n╰───────╯";
    let output = cleaner.clean(input);
    assert_eq!(output, "\n hello \n");
}

/// Claude spinner characters are stripped when strip_spinner_chars is enabled.
/// Replicates `strips_claude_spinners`.
#[test]
fn claude_spinner_characters_are_stripped() {
    let mut cleaner = default_cleaner();
    let input = "✻ Thinking... ✳ Working... ✶ Done";
    let output = cleaner.clean(input);
    assert_eq!(output, " Thinking...  Working...  Done");
}

/// Gemini braille spinner characters are stripped.
/// Replicates `strips_gemini_braille_spinners`.
#[test]
fn gemini_braille_spinners_are_stripped() {
    let mut cleaner = default_cleaner();
    let input = "⠋ Loading ⠙ Loading ⠹ Loading";
    let output = cleaner.clean(input);
    assert_eq!(output, " Loading  Loading  Loading");
}

/// Progress bar block characters are stripped when strip_progress_blocks is enabled.
/// Replicates `strips_progress_blocks`.
#[test]
fn progress_block_characters_are_stripped() {
    let mut cleaner = default_cleaner();
    let input = "Progress: ████░░░░ 50%";
    let output = cleaner.clean(input);
    assert_eq!(output, "Progress:  50%");
}

/// Color inside cursor movement is correctly handled (nested sequences).
/// Replicates `handles_nested_sequences`.
#[test]
fn nested_escape_sequences_are_handled() {
    let mut cleaner = default_cleaner();
    let input = "\x1b[2K\x1b[38;5;174mtext\x1b[0m\x1b[1G";
    let output = cleaner.clean(input);
    assert_eq!(output, "text");
}

/// An incomplete CSI at end of input must not produce garbage.
/// Replicates `handles_partial_sequences`.
#[test]
fn partial_escape_sequence_at_end_is_handled() {
    let mut cleaner = default_cleaner();
    let input = "hello\x1b[";
    let output = cleaner.clean(input);
    assert_eq!(output, "hello");
}

// ---------------------------------------------------------------------------
// Additional edge-case tests
// ---------------------------------------------------------------------------

/// The output is never longer than the input (size invariant).
#[test]
fn output_never_exceeds_input_size() {
    let mut cleaner = default_cleaner();
    let inputs = [
        "hello world",
        "\x1b[31mred text\x1b[0m",
        "\x1b[2K\x1b[1A\x1b[G moving around",
        "\u{2500}\u{2502}\u{256D}\u{256E} box chars",
        "\u{280B}\u{2819}\u{2839} spinner chars",
        "",
    ];
    for input in inputs {
        let output = cleaner.clean(input);
        assert!(
            output.len() <= input.len(),
            "output ({}) should not exceed input ({}) for {:?}",
            output.len(),
            input.len(),
            input
        );
    }
}

/// Cleaning twice must yield the same result as cleaning once (idempotent).
#[test]
fn clean_is_idempotent() {
    let mut cleaner = default_cleaner();
    let inputs = [
        "\x1b[31mcolored\x1b[0m text",
        "plain text only",
        "\u{2500}\u{2502} box \u{2713} check",
    ];
    for input in inputs {
        let once = cleaner.clean(input);
        let twice = cleaner.clean(&once);
        assert_eq!(
            once, twice,
            "Cleaning should be idempotent for {:?}: first='{}', second='{}'",
            input, once, twice
        );
    }
}

/// The `transform()` implementation applies `clean()` only to output events.
#[test]
fn transform_only_cleans_output_events() {
    use agr::asciicast::Event;

    let mut cleaner = default_cleaner();
    let raw_ansi = "\x1b[31mcolored\x1b[0m";

    let mut events = vec![
        Event::output(0.1, raw_ansi),
        Event::marker(0.1, raw_ansi), // marker data must NOT be cleaned
    ];
    cleaner.transform(&mut events);

    // Output event: ANSI stripped
    assert!(
        !events[0].data.contains('\x1b'),
        "ANSI in output event should be stripped"
    );
    assert_eq!(
        events[0].data, "colored",
        "Cleaned output should equal 'colored'"
    );

    // Marker event: data unchanged
    assert_eq!(
        events[1].data, raw_ansi,
        "Marker data should not be modified"
    );
}

/// ANSI stripped counter increases with each sequence processed.
#[test]
fn ansi_stripped_count_increases() {
    let mut cleaner = default_cleaner();
    cleaner.clean("\x1b[31mred\x1b[0m");
    assert!(
        cleaner.ansi_stripped_count() >= 1,
        "ANSI stripped count should be >= 1"
    );
}

/// Control character stripped counter increases correctly.
#[test]
fn control_stripped_count_increases() {
    let mut cleaner = default_cleaner();
    cleaner.clean("hello\x07\x00world");
    assert!(
        cleaner.control_stripped_count() >= 1,
        "Control stripped count should be >= 1"
    );
}

/// After reset_stats, both counters return to zero.
#[test]
fn reset_stats_zeroes_all_counters() {
    let mut cleaner = default_cleaner();
    cleaner.clean("\x1b[31mred\x1b[0m hello\x07world");
    cleaner.reset_stats();
    assert_eq!(
        cleaner.ansi_stripped_count(),
        0,
        "ANSI count must be 0 after reset"
    );
    assert_eq!(
        cleaner.control_stripped_count(),
        0,
        "control count must be 0 after reset"
    );
}

/// DEL (0x7F) is treated as a control character and stripped.
#[test]
fn del_character_is_stripped() {
    let mut cleaner = default_cleaner();
    let output = cleaner.clean("hello\x7fworld");
    assert_eq!(output, "helloworld");
}

/// C1 control characters (0x80-0x9F) are stripped.
#[test]
fn c1_control_characters_are_stripped() {
    let mut cleaner = default_cleaner();
    // 0x80 and 0x9F are C1 control chars
    let input = "hello\u{0080}world\u{009F}end";
    let output = cleaner.clean(input);
    assert_eq!(output, "helloworldend");
}

/// Simple ESC sequences (ESC followed by a single alphabetic char) are stripped.
#[test]
fn simple_esc_alphabetic_sequences_are_stripped() {
    let mut cleaner = default_cleaner();
    // ESC c = reset terminal, ESC 7 and ESC ( are common
    let input = "\x1bctext\x1b(more";
    let output = cleaner.clean(input);
    // "text" and "more" must survive; ESC sequences stripped
    assert!(
        output.contains("text"),
        "Text after simple ESC must survive"
    );
    assert!(output.contains("more"), "Text after ESC ( must survive");
}

/// Empty input yields empty output.
#[test]
fn empty_input_yields_empty_output() {
    let mut cleaner = default_cleaner();
    assert_eq!(cleaner.clean(""), "");
}

/// Input with no escape sequences is returned unchanged.
#[test]
fn plain_text_is_returned_unchanged() {
    let mut cleaner = default_cleaner();
    let input = "Hello, World! This is plain text 123.";
    assert_eq!(cleaner.clean(input), input);
}
