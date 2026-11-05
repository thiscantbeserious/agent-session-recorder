//! Rendering helper and preview tests.

use crate::tui::widgets::file_explorer::render_helpers::*;
use crate::tui::widgets::file_explorer::*;
use chrono::TimeZone;
use ratatui::text::Line;

// -------------------------------------------------------------------------
// build_rename_item_spans tests
// -------------------------------------------------------------------------

fn test_theme() -> crate::theme::Theme {
    crate::theme::Theme::claude_code()
}

/// selected_all=true: entire input is rendered as a single highlighted span
/// followed by ".cast" and the agent/size metadata.
#[test]
fn rename_spans_selected_all() {
    let theme = test_theme();
    let spans = build_rename_item_spans("my-session", 0, true, "claude", "1.0 KB", &theme);
    // First span should contain the full input text
    assert_eq!(spans[0].content, "my-session");
    // Second span contains ".cast"
    assert!(spans.iter().any(|s| s.content.contains(".cast")));
    // Metadata span present
    assert!(spans.iter().any(|s| s.content.contains("claude")));
}

/// cursor at position 0 (start of string): cursor block on first char.
#[test]
fn rename_spans_cursor_start() {
    let theme = test_theme();
    let spans = build_rename_item_spans("abc", 0, false, "ag", "2 B", &theme);
    // No "before" span at cursor=0; first span is the cursor char
    assert_eq!(spans[0].content, "a");
    // After portion
    assert!(spans.iter().any(|s| s.content == "bc"));
    assert!(spans.iter().any(|s| s.content.contains(".cast")));
}

/// cursor in the middle of the string.
#[test]
fn rename_spans_cursor_middle() {
    let theme = test_theme();
    let spans = build_rename_item_spans("hello", 2, false, "ag", "0 B", &theme);
    // "he" before cursor, "l" cursor char, "lo" after
    assert!(spans.iter().any(|s| s.content == "he"));
    assert!(spans.iter().any(|s| s.content == "l"));
    assert!(spans.iter().any(|s| s.content == "lo"));
}

/// cursor at end of string: "." of ".cast" becomes the cursor block.
#[test]
fn rename_spans_cursor_end() {
    let theme = test_theme();
    let spans = build_rename_item_spans("hi", 2, false, "ag", "0 B", &theme);
    // "hi" is the before span
    assert!(spans.iter().any(|s| s.content == "hi"));
    // cursor char is "."
    assert!(spans.iter().any(|s| s.content == "."));
    // remainder is "cast"
    assert!(spans.iter().any(|s| s.content == "cast"));
}

/// empty input with cursor at 0 behaves like cursor-at-end.
#[test]
fn rename_spans_empty_input() {
    let theme = test_theme();
    let spans = build_rename_item_spans("", 0, false, "ag", "0 B", &theme);
    // Should not panic and must still produce a cursor block on "."
    assert!(spans.iter().any(|s| s.content == "."));
    assert!(spans.iter().any(|s| s.content == "cast"));
}

/// Unicode: cursor positioned at a multi-byte character boundary.
#[test]
fn rename_spans_unicode_cursor() {
    let theme = test_theme();
    // 'é' is two bytes (0xC3 0xA9) in UTF-8
    let input = "héllo";
    let cursor = 1; // byte position of 'é'
    let spans = build_rename_item_spans(input, cursor, false, "ag", "0 B", &theme);
    // "h" before cursor
    assert!(spans.iter().any(|s| s.content == "h"));
    // cursor char is "é"
    assert!(spans.iter().any(|s| s.content == "é"));
    // after is "llo"
    assert!(spans.iter().any(|s| s.content == "llo"));
}

/// cursor beyond end of string: clamped to end, no panic.
#[test]
fn rename_spans_cursor_beyond_end_does_not_panic() {
    let theme = test_theme();
    let input = "hi";
    // cursor=999 is way beyond the end; must be clamped to len()=2
    let spans = build_rename_item_spans(input, 999, false, "ag", "0 B", &theme);
    // Should behave like cursor-at-end: "hi" before, "." cursor block, "cast" suffix
    assert!(spans.iter().any(|s| s.content == "hi"));
    assert!(spans.iter().any(|s| s.content == "."));
    assert!(spans.iter().any(|s| s.content == "cast"));
}

/// cursor inside a multi-byte UTF-8 char (not on a boundary): snapped back
/// to the preceding valid boundary, no panic.
#[test]
fn rename_spans_cursor_mid_multibyte_snaps_to_boundary() {
    let theme = test_theme();
    // 'é' occupies bytes 1..3; passing cursor=2 (middle of 'é') must not panic
    let input = "héllo";
    let spans = build_rename_item_spans(input, 2, false, "ag", "0 B", &theme);
    // Snapped back to byte 1 (start of 'é'), so "h" is before and "é" is the cursor char
    assert!(spans.iter().any(|s| s.content == "h"));
    assert!(spans.iter().any(|s| s.content == "é"));
}

// -------------------------------------------------------------------------
// build_normal_item_spans tests
// -------------------------------------------------------------------------

/// Unlocked item: name rendered with primary text style, no camera emoji.
#[test]
fn normal_spans_unlocked() {
    let theme = test_theme();
    let spans = build_normal_item_spans("session.cast", "claude", "1.0 KB", false, &theme);
    // First span is the file name
    assert_eq!(spans[0].content, "session.cast");
    // No camera emoji in any span
    assert!(!spans.iter().any(|s| s.content.contains('\u{1F4F9}')));
    // Metadata span present
    assert!(spans.iter().any(|s| s.content.contains("claude")));
}

/// Locked item: camera emoji appended, secondary style used for name.
#[test]
fn normal_spans_locked() {
    let theme = test_theme();
    let spans = build_normal_item_spans("live.cast", "codex", "512 B", true, &theme);
    // Name is still the first span
    assert_eq!(spans[0].content, "live.cast");
    // Camera emoji present
    assert!(spans.iter().any(|s| s.content.contains('\u{1F4F9}')));
}

// -------------------------------------------------------------------------
// append_session_preview_lines tests
// -------------------------------------------------------------------------

/// Empty styled_preview: no "Preview" header or snapshot lines are added.
#[test]
fn append_preview_lines_empty_preview() {
    let theme = test_theme();
    let mut lines: Vec<Line<'static>> = vec![];
    let modified = chrono::Local::now();

    append_session_preview_lines(
        &mut lines,
        "5m 10s".to_string(),
        3,
        vec![],
        modified,
        None,
        false,
        &theme,
    );

    // Duration and Markers lines must be present
    let content: Vec<String> = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
        .collect();
    assert!(content.iter().any(|s| s == "Duration: "));
    assert!(content.iter().any(|s| s == "5m 10s"));
    assert!(content.iter().any(|s| s == "Markers: "));
    assert!(content.iter().any(|s| s == "3"));
    // No "Preview" header since styled_preview is empty
    assert!(!content.iter().any(|s| s == "Preview"));
}

/// Backup available + lock info: both status lines are included.
#[test]
fn append_preview_lines_backup_and_lock() {
    let theme = test_theme();
    let mut lines: Vec<Line<'static>> = vec![];
    let modified = chrono::Local::now();
    let lock = crate::files::lock::LockInfo {
        pid: 12345,
        started: "2024-01-01T00:00:00Z".to_string(),
    };

    append_session_preview_lines(
        &mut lines,
        "1m 0s".to_string(),
        0,
        vec![],
        modified,
        Some(lock),
        true,
        &theme,
    );

    let content: Vec<String> = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
        .collect();
    assert!(content.iter().any(|s| s == "Backup: "));
    assert!(content.iter().any(|s| s == "Available"));
    assert!(content.iter().any(|s| s == "Status: "));
    assert!(content.iter().any(|s| s.contains("12345")));
}

/// Styled preview capped at 12 lines even if more are provided.
#[test]
fn append_preview_lines_capped_at_12() {
    use crate::terminal::{Cell, CellStyle, StyledLine};

    let theme = test_theme();
    let modified = chrono::Local::now();

    // Build 20 styled lines each containing a single space character
    let make_line = || StyledLine {
        cells: vec![Cell {
            char: 'x',
            style: CellStyle::default(),
        }],
    };
    let styled_preview: Vec<StyledLine> = (0..20).map(|_| make_line()).collect();

    let mut lines: Vec<Line<'static>> = vec![];
    append_session_preview_lines(
        &mut lines,
        "0s".to_string(),
        0,
        styled_preview,
        modified,
        None,
        false,
        &theme,
    );

    // Count how many lines contain the 'x' content (padded to " x")
    let snapshot_lines = lines
        .iter()
        .filter(|l| {
            l.spans
                .iter()
                .any(|s| s.content.contains('x') || s.content.trim().contains('x'))
        })
        .count();
    assert!(
        snapshot_lines <= 12,
        "snapshot lines {snapshot_lines} must be <= 12"
    );
}

// -------------------------------------------------------------------------
// build_preview_lines tests
// -------------------------------------------------------------------------

/// No session preview data -> fallback Modified + Path lines rendered.
#[test]
fn preview_lines_no_session_data_shows_fallback() {
    let theme = test_theme();
    let modified = chrono::Local
        .with_ymd_and_hms(2024, 6, 1, 12, 0, 0)
        .unwrap();

    let lines = build_preview_lines(
        "test.cast".to_string(),
        "claude".to_string(),
        2048,
        modified,
        "/sessions/test.cast".to_string(),
        None,
        None, // no session_preview_data
        false,
        &theme,
    );

    let content: Vec<String> = lines
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
        .collect();

    // Basic fields
    assert!(content.iter().any(|s| s == "Name: "));
    assert!(content.iter().any(|s| s == "test.cast"));
    assert!(content.iter().any(|s| s == "Agent: "));
    assert!(content.iter().any(|s| s == "claude"));
    // Fallback fields (no session preview -> Modified + Path appear)
    assert!(content.iter().any(|s| s == "Modified: "));
    assert!(content.iter().any(|s| s == "Path: "));
    // Duration must NOT appear (no session data)
    assert!(!content.iter().any(|s| s == "Duration: "));
}

// SessionPreview tests

#[test]
fn session_preview_format_duration_seconds() {
    let preview = SessionPreview {
        duration_secs: 45.0,
        marker_count: 0,
        styled_preview: Vec::new(),
    };
    assert_eq!(preview.format_duration(), "45s");
}

#[test]
fn session_preview_format_duration_minutes() {
    let preview = SessionPreview {
        duration_secs: 332.0, // 5m 32s
        marker_count: 0,
        styled_preview: Vec::new(),
    };
    assert_eq!(preview.format_duration(), "5m 32s");
}

#[test]
fn session_preview_format_duration_hours() {
    let preview = SessionPreview {
        duration_secs: 3732.0, // 1h 2m 12s
        marker_count: 0,
        styled_preview: Vec::new(),
    };
    assert_eq!(preview.format_duration(), "1h 2m 12s");
}
