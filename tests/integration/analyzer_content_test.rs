//! Integration tests for analyzer content extraction pipeline.
//!
//! These tests verify the content cleaning transforms work correctly
//! on realistic input data derived from actual agent sessions.

use agr::analyzer::{
    ContentCleaner, DeduplicateProgressLines, ExtractionConfig, FilterEmptyEvents,
    NormalizeWhitespace,
};
use agr::asciicast::{Event, Transform};

// ============================================================================
// Snapshot Tests for ContentCleaner
// ============================================================================

#[test]
fn snapshot_claude_box_drawing_cleanup() {
    let config = ExtractionConfig::default();
    let mut cleaner = ContentCleaner::new(&config);

    // Claude's box-drawn API request dialog (from SPEC.md Section 1.7)
    let raw = concat!(
        "\x1b[?2026h\x1b[16;3H\x1b[0m\x1b[38;5;174m\x1b[1m  \u{256D}\u{2500}",
        "\x1b[0m\x1b[38;5;174m\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "\x1b[48;5;174m\x1b[38;5;16m API Request \x1b[0m\x1b[38;5;174m",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "\x1b[1m\u{2500}\u{256E}\x1b[0m\n",
        "\x1b[38;5;174m\x1b[1m  \u{2502}\x1b[0m\x1b[38;5;174m ",
        "This tool call will make an API request   \x1b[1m \u{2502}\x1b[0m\n",
        "\x1b[38;5;174m\x1b[1m  \u{2502}\x1b[0m\x1b[38;5;174m  ",
        "POST https://api.anthropic.com/v1/messages\x1b[1m \u{2502}\x1b[0m\n",
        "\x1b[38;5;174m\x1b[1m  \u{256E}\u{2500}\x1b[0m\x1b[38;5;174m",
        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
        "\x1b[1m\u{2500}\u{256F}\x1b[0m"
    );

    let clean = cleaner.clean(raw);

    // Should extract just the semantic content
    insta::assert_snapshot!("claude_box_drawing", clean);
}

#[test]
fn snapshot_codex_menu_cleanup() {
    let config = ExtractionConfig::default();
    let mut cleaner = ContentCleaner::new(&config);

    // Codex menu selection (from SPEC.md Section 1.7)
    let raw = concat!(
        "\x1b[?2026h\x1b[1;61H\x1b[0m\x1b[49m\x1b[K\x1b[?25l",
        "\x1b[1;61H\x1b[48;5;234m\x1b[38;5;7m",
        "\x1b[2m\x1b[38;5;8m \x1b[22m",
        "1. Allow Codex to work in this folder without asking for approval",
        "\x1b[2m\x1b[38;5;8m \x1b[22m",
        "  2. Require approval of edits and commands",
        "\x1b[?25h\x1b[?2026l"
    );

    let clean = cleaner.clean(raw);

    insta::assert_snapshot!("codex_menu", clean);
}

#[test]
fn snapshot_gemini_prompt_cleanup() {
    let config = ExtractionConfig::default();
    let mut cleaner = ContentCleaner::new(&config);

    // Gemini prompt input (from SPEC.md Section 1.7)
    let raw = concat!(
        "\x1b[2K\x1b[1A\x1b[2K\x1b[1A\x1b[2K\x1b[G",
        "\x1b[38;5;35m?\x1b[39m \x1b[1mEnter your message\x1b[22m",
        "\x1b[38;5;239m (Ctrl+C to quit)\x1b[39m\x1b[57G",
        "\x1b[38;5;239m\x1b[39m\x1b[G\x1b[2K\x1b[1A\x1b[2K\x1b[G",
        "\x1b[38;5;6m>\x1b[39m can you understand the current project? ",
        "i want to have a detailed session to\n",
        "  have all kind of weird output that you can produce"
    );

    let clean = cleaner.clean(raw);

    insta::assert_snapshot!("gemini_prompt", clean);
}

// ============================================================================
// Snapshot Tests for DeduplicateProgressLines
// ============================================================================

#[test]
fn snapshot_spinner_progress_dedupe() {
    let mut deduper = DeduplicateProgressLines::new();

    // Spinner progress sequence (common pattern in all agents)
    let mut events = vec![
        Event::output(0.1, "\r\u{280B} Building..."),      // ⠋
        Event::output(0.1, "\r\u{2819} Building..."),      // ⠙
        Event::output(0.1, "\r\u{2839} Building..."),      // ⠹
        Event::output(0.1, "\r\u{2838} Building..."),      // ⠸
        Event::output(0.1, "\r\u{283C} Building..."),      // ⠼
        Event::output(0.1, "\r\u{2713} Build complete\n"), // ✓
    ];

    deduper.transform(&mut events);

    // Format events for snapshot
    let output: String = events
        .iter()
        .map(|e| format!("[{:.1}] {}", e.time, e.data.escape_debug()))
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!("spinner_progress_dedupe", output);
}

#[test]
fn snapshot_mixed_progress_and_output() {
    let mut deduper = DeduplicateProgressLines::new();

    let mut events = vec![
        Event::output(0.1, "Starting build...\n"),
        Event::output(0.1, "\r[1/5] Compiling..."),
        Event::output(0.1, "\r[2/5] Compiling..."),
        Event::output(0.1, "\r[3/5] Compiling..."),
        Event::output(0.1, "\r[4/5] Compiling..."),
        Event::output(0.1, "\r[5/5] Done\n"),
        Event::marker(0.1, "build-complete"),
        Event::output(0.1, "Running tests...\n"),
    ];

    deduper.transform(&mut events);

    let output: String = events
        .iter()
        .map(|e| {
            if e.is_marker() {
                format!("[M] {}", e.data)
            } else {
                format!("[{:.1}] {}", e.time, e.data.escape_debug())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!("mixed_progress_and_output", output);
}

// ============================================================================
// Full Pipeline Integration Test
// ============================================================================

#[test]
fn snapshot_full_pipeline_claude() {
    // Simulate a Claude session excerpt with box drawing, colors, and content
    let mut events = vec![
        // Box drawing header
        Event::output(
            0.1,
            concat!(
                "\x1b[38;5;174m\u{256D}\u{2500}\u{2500}\u{2500} Task \u{2500}\u{2500}\u{2500}\u{256E}\n",
                "\x1b[38;5;174m\u{2502} Analyzing code... \u{2502}\n",
                "\x1b[38;5;174m\u{2570}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{256F}\n"
            ),
        ),
        // Spinner progress
        Event::output(0.1, "\r\u{273B} Thinking..."),
        Event::output(0.1, "\r\u{2733} Thinking..."),
        Event::output(0.1, "\r\u{2713} Analysis complete\n"),
        // Content with semantic markers
        Event::output(0.2, "\x1b[32m\u{2714} Tests passed\x1b[0m\n"),
        Event::output(0.1, "\x1b[31m\u{2715} Build failed\x1b[0m\n"),
        Event::output(0.1, "\x1b[33m\u{26A0} Warning: deprecated API\x1b[0m\n"),
    ];

    // Apply full pipeline
    let config = ExtractionConfig::default();
    let mut cleaner = ContentCleaner::new(&config);
    cleaner.transform(&mut events);

    let mut deduper = DeduplicateProgressLines::new();
    deduper.transform(&mut events);

    let mut normalizer = NormalizeWhitespace::new(2);
    normalizer.transform(&mut events);

    FilterEmptyEvents.transform(&mut events);

    // Format output
    let output: String = events
        .iter()
        .map(|e| e.data.clone())
        .collect::<Vec<_>>()
        .join("");

    insta::assert_snapshot!("full_pipeline_claude", output);
}

// ============================================================================
// Property-based invariants
// ============================================================================

#[test]
fn content_cleaner_never_increases_size() {
    let config = ExtractionConfig::default();
    let mut cleaner = ContentCleaner::new(&config);

    let inputs = [
        "hello world",
        "\x1b[31mred text\x1b[0m",
        "no escapes here",
        "\x1b[2K\x1b[1A\x1b[G moving around",
        "\u{2500}\u{2502}\u{256D}\u{256E} box chars",
        "\u{280B}\u{2819}\u{2839} spinner chars",
        "",
    ];

    for input in inputs {
        let output = cleaner.clean(input);
        assert!(
            output.len() <= input.len(),
            "Output {} should not be longer than input {}",
            output.len(),
            input.len()
        );
    }
}

#[test]
fn content_cleaner_is_idempotent() {
    let config = ExtractionConfig::default();
    let mut cleaner = ContentCleaner::new(&config);

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
            "Cleaning should be idempotent: first pass '{}', second pass '{}'",
            once, twice
        );
    }
}

#[test]
fn dedupe_transform_preserves_all_markers() {
    let mut deduper = DeduplicateProgressLines::new();

    let mut events = vec![
        Event::output(0.1, "content\n"),
        Event::marker(0.1, "marker1"),
        Event::output(0.1, "\rprogress1"),
        Event::output(0.1, "\rprogress2"),
        Event::marker(0.1, "marker2"),
        Event::output(0.1, "\rfinal\n"),
        Event::marker(0.1, "marker3"),
    ];

    let original_marker_count = events.iter().filter(|e| e.is_marker()).count();

    deduper.transform(&mut events);

    let final_marker_count = events.iter().filter(|e| e.is_marker()).count();

    assert_eq!(
        original_marker_count, final_marker_count,
        "All markers should be preserved"
    );

    // Verify marker labels
    let marker_labels: Vec<_> = events
        .iter()
        .filter(|e| e.is_marker())
        .map(|e| e.data.as_str())
        .collect();

    assert_eq!(marker_labels, vec!["marker1", "marker2", "marker3"]);
}

#[test]
fn filter_empty_preserves_all_non_output_events() {
    let mut events = vec![
        Event::output(0.1, ""),                        // Empty - remove
        Event::marker(0.1, "marker"),                  // Keep
        Event::output(0.1, "   \n\t  "),               // Has spaces - keep (TUI preserves spaces)
        Event::new(0.1, agr::EventType::Input, "key"), // Keep
        Event::output(0.1, "content"),                 // Keep
    ];

    FilterEmptyEvents.transform(&mut events);

    // FilterEmptyEvents keeps events containing spaces (for TUI compatibility)
    assert_eq!(events.len(), 4);
    assert!(events[0].is_marker());
    assert!(events[1].is_output()); // "   \n\t  " kept because it contains spaces
    assert_eq!(events[2].event_type, agr::EventType::Input);
    assert!(events[3].is_output());
}
