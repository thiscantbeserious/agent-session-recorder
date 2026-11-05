//! Unit tests for the list app rendering helpers.

use super::render::{
    build_menu_item_label, build_rename_status_spans, format_duration, menu_item_style,
};
use super::ContextMenuItem;
use ratatui::style::{Modifier, Style};
// ── build_menu_item_label ─────────────────────────────────────────────

#[test]
fn menu_item_label_normal_item_with_shortcut() {
    // Play has shortcut "p"
    let label = build_menu_item_label(&ContextMenuItem::Play, true);
    assert!(label.contains("Play"));
    assert!(label.contains("(p)"));
    assert!(!label.contains("no backup"));
}

#[test]
fn menu_item_label_restore_with_backup() {
    let label = build_menu_item_label(&ContextMenuItem::Restore, true);
    assert!(label.contains("Restore"));
    assert!(!label.contains("no backup"));
}

#[test]
fn menu_item_label_restore_no_backup() {
    let label = build_menu_item_label(&ContextMenuItem::Restore, false);
    assert!(label.contains("Restore"));
    assert!(label.contains("no backup"));
}

#[test]
fn menu_item_label_copy_item() {
    let label = build_menu_item_label(&ContextMenuItem::Copy, true);
    assert!(label.contains("Copy to clipboard"));
    assert!(label.contains("(c)"));
}

// ── menu_item_style ───────────────────────────────────────────────────

#[test]
fn menu_item_style_selected_uses_highlight() {
    let theme = crate::theme::Theme::claude_code();
    let style = menu_item_style(true, false, &theme);
    let expected = theme.highlight_style();
    assert_eq!(style, expected);
}

#[test]
fn menu_item_style_disabled_uses_secondary_color() {
    let theme = crate::theme::Theme::claude_code();
    let style = menu_item_style(false, true, &theme);
    assert_eq!(style.fg, Some(theme.text_secondary));
}

#[test]
fn menu_item_style_normal_uses_primary_color() {
    let theme = crate::theme::Theme::claude_code();
    let style = menu_item_style(false, false, &theme);
    assert_eq!(style.fg, Some(theme.text_primary));
}

#[test]
fn menu_item_style_selected_takes_precedence_over_disabled() {
    // selected=true, disabled=true → still highlight (selected wins)
    let theme = crate::theme::Theme::claude_code();
    let style = menu_item_style(true, true, &theme);
    let expected = theme.highlight_style();
    assert_eq!(style, expected);
}

// ── build_rename_status_spans ─────────────────────────────────────────

#[test]
fn rename_spans_selected_all_wraps_in_brackets() {
    let hl = ratatui::style::Style::default();
    let blink = ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
    let mut spans = vec![];
    build_rename_status_spans(&mut spans, "hello", 0, true, hl, blink);
    // Expect 3 spans: "[", "hello", "]" — all with blink style
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].content, "[");
    assert_eq!(spans[1].content, "hello");
    assert_eq!(spans[2].content, "]");
    assert_eq!(spans[0].style, blink);
}

#[test]
fn rename_spans_cursor_at_start_no_before_span() {
    let hl = ratatui::style::Style::default();
    let blink = ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
    let mut spans = vec![];
    // cursor=0, selected_all=false → only "|" + "hello"
    build_rename_status_spans(&mut spans, "hello", 0, false, hl, blink);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].content, "|");
    assert_eq!(spans[1].content, "hello");
}

#[test]
fn rename_spans_cursor_at_end_no_after_span() {
    let hl = ratatui::style::Style::default();
    let blink = ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
    let mut spans = vec![];
    // cursor at end → "hello" + "|"
    build_rename_status_spans(&mut spans, "hello", 5, false, hl, blink);
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].content, "hello");
    assert_eq!(spans[1].content, "|");
}

#[test]
fn rename_spans_cursor_in_middle_three_spans() {
    let hl = ratatui::style::Style::default();
    let blink = ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
    let mut spans = vec![];
    // cursor=2 in "hello" → "he" + "|" + "llo"
    build_rename_status_spans(&mut spans, "hello", 2, false, hl, blink);
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].content, "he");
    assert_eq!(spans[1].content, "|");
    assert_eq!(spans[2].content, "llo");
}

#[test]
fn rename_spans_empty_input_cursor_only() {
    let hl = ratatui::style::Style::default();
    let blink = ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
    let mut spans = vec![];
    // empty input → only "|"
    build_rename_status_spans(&mut spans, "", 0, false, hl, blink);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].content, "|");
}

#[test]
fn format_duration_negative_input_clamps_to_zero() {
    assert_eq!(format_duration(-5.0), "0s");
}

#[test]
fn format_duration_nan_clamps_to_zero() {
    assert_eq!(format_duration(f64::NAN), "0s");
}

#[test]
fn rename_spans_multibyte_cursor_at_char_boundary() {
    let theme = crate::theme::Theme::claude_code();
    let hl = theme.highlight_style();
    let blink = Style::default().add_modifier(Modifier::SLOW_BLINK);
    let mut spans = vec![];
    // é is 2 bytes in UTF-8
    let input = "\u{00e9}llo"; // 4 bytes total
    build_rename_status_spans(&mut spans, input, 2, false, hl, blink);
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].content, "\u{00e9}");
    assert_eq!(spans[1].content, "|");
    assert_eq!(spans[2].content, "llo");
}
