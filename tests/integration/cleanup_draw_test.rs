//! Behavior-preserving tests for extracted pure functions from `CleanupApp::draw()`.
//!
//! Tests cover `footer_text_for_mode()` and `status_line_content()` which are
//! free functions extracted from `draw()` and `handle_mouse()`.

use agr::tui::cleanup_app::{footer_text_for_mode, status_line_content, Mode};

// ============================================================================
// footer_text_for_mode
// ============================================================================

#[test]
fn footer_text_for_search_mode() {
    let text = footer_text_for_mode(Mode::Search, 0);
    assert_eq!(text, "Esc: cancel | Enter: apply | Backspace: delete");
}

#[test]
fn footer_text_for_glob_select_mode() {
    let text = footer_text_for_mode(Mode::GlobSelect, 0);
    assert_eq!(
        text,
        "Esc: cancel | Enter: select matching | Backspace: delete"
    );
}

#[test]
fn footer_text_for_agent_filter_mode() {
    let text = footer_text_for_mode(Mode::AgentFilter, 0);
    assert_eq!(text, "left/right: change | Enter: apply | Esc: cancel");
}

#[test]
fn footer_text_for_confirm_delete_mode() {
    let text = footer_text_for_mode(Mode::ConfirmDelete, 3);
    assert_eq!(text, "y: confirm delete | n/Esc: cancel");
}

#[test]
fn footer_text_for_confirm_delete_final_mode() {
    let text = footer_text_for_mode(Mode::ConfirmDeleteFinal, 3);
    assert_eq!(text, "y: confirm | n/Esc: cancel");
}

#[test]
fn footer_text_for_help_mode() {
    let text = footer_text_for_mode(Mode::Help, 0);
    assert_eq!(text, "Press any key to close");
}

#[test]
fn footer_text_normal_mode_no_selection() {
    let text = footer_text_for_mode(Mode::Normal, 0);
    assert_eq!(
        text,
        "Space: select | a: all | g: glob | /: search | f: filter | ?: help | q: quit"
    );
}

#[test]
fn footer_text_normal_mode_with_selection() {
    let text = footer_text_for_mode(Mode::Normal, 2);
    assert_eq!(
        text,
        "Space: toggle | a: toggle all | Enter: delete selected | Esc: clear | ?: help"
    );
}

#[test]
fn footer_text_normal_mode_selection_count_boundary() {
    // selected_count = 1 triggers the "has selection" branch
    let text_with = footer_text_for_mode(Mode::Normal, 1);
    let text_without = footer_text_for_mode(Mode::Normal, 0);
    assert_ne!(
        text_with, text_without,
        "footer text should differ when selection exists"
    );
}

// ============================================================================
// status_line_content
// ============================================================================

#[test]
fn status_line_uses_status_message_when_set() {
    let status = Some("Custom message".to_string());
    let text = status_line_content(Mode::Normal, &status, 0, 0, 5, None, None);
    assert_eq!(text, "Custom message");
}

#[test]
fn status_line_normal_no_selection_no_filters() {
    let text = status_line_content(Mode::Normal, &None, 0, 0, 42, None, None);
    assert_eq!(text, "42 sessions | Space to select");
}

#[test]
fn status_line_normal_with_selection() {
    // 1024 bytes = 1 KiB in binary
    let text = status_line_content(Mode::Normal, &None, 3, 1024, 42, None, None);
    assert!(text.contains("3 selected"), "should show selected count");
    assert!(
        text.contains("42 total sessions"),
        "should show total count"
    );
}

#[test]
fn status_line_normal_with_search_filter() {
    let text = status_line_content(Mode::Normal, &None, 0, 0, 5, Some("claude"), None);
    assert!(
        text.contains("search: \"claude\""),
        "should show search filter"
    );
    assert!(text.contains("5 sessions"), "should show session count");
}

#[test]
fn status_line_normal_with_agent_filter() {
    let text = status_line_content(Mode::Normal, &None, 0, 0, 3, None, Some("codex"));
    assert!(text.contains("agent: codex"), "should show agent filter");
    assert!(text.contains("3 sessions"), "should show session count");
}

#[test]
fn status_line_normal_with_both_filters() {
    let text = status_line_content(Mode::Normal, &None, 0, 0, 2, Some("test"), Some("claude"));
    assert!(
        text.contains("search: \"test\""),
        "should contain search filter"
    );
    assert!(
        text.contains("agent: claude"),
        "should contain agent filter"
    );
    assert!(text.contains("2 sessions"), "should show session count");
}

#[test]
fn status_line_other_mode_returns_empty() {
    let text = status_line_content(Mode::Help, &None, 0, 0, 5, None, None);
    assert_eq!(text, "", "non-Normal mode without status should be empty");
}

#[test]
fn status_line_status_message_overrides_normal_content() {
    // Even in Normal with selection, a status_message takes priority
    let status = Some("Files deleted!".to_string());
    let text = status_line_content(Mode::Normal, &status, 5, 2048, 10, None, None);
    assert_eq!(
        text, "Files deleted!",
        "status_message overrides normal content"
    );
}
