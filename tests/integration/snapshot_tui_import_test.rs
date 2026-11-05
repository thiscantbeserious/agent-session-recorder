//! Visual snapshot tests for lock icons, status bar dialog states and import modals.

use crate::helpers::tui_snapshot::{
    assert_explorer_snapshot, render_explorer_to_string, render_explorer_with_preview, snap,
};
use agr::files::lock::LockInfo;
use agr::tui::widgets::{FileExplorer, FileItem};
use chrono::{Local, TimeZone};
use ratatui::layout::Rect;

/// Create test file items with one item actively being recorded (locked).
fn create_test_file_items_with_lock() -> Vec<FileItem> {
    let mut items = vec![
        FileItem {
            path: "/sessions/claude/20240115-session1.cast".to_string(),
            name: "20240115-session1.cast".to_string(),
            agent: "claude".to_string(),
            size: 1024 * 50,
            modified: Local.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap(),
            has_backup: false,
            lock_info: None,
        },
        FileItem {
            path: "/sessions/claude/20240116-recording.cast".to_string(),
            name: "20240116-recording.cast".to_string(),
            agent: "claude".to_string(),
            size: 1024 * 10,
            modified: Local.with_ymd_and_hms(2024, 1, 16, 14, 0, 0).unwrap(),
            has_backup: false,
            lock_info: Some(LockInfo {
                pid: 12345,
                started: "2024-01-16T14:00:00".to_string(),
            }),
        },
        FileItem {
            path: "/sessions/codex/20240114-session3.cast".to_string(),
            name: "20240114-session3.cast".to_string(),
            agent: "codex".to_string(),
            size: 1024 * 100,
            modified: Local.with_ymd_and_hms(2024, 1, 14, 9, 0, 0).unwrap(),
            has_backup: false,
            lock_info: None,
        },
    ];
    // Sort by date descending (default) - the locked item should be first
    items.sort_by_key(|b| std::cmp::Reverse(b.modified));
    items
}

#[test]
fn snapshot_file_explorer_with_locked_item() {
    let mut explorer = FileExplorer::new(create_test_file_items_with_lock());

    let output = render_explorer_to_string(&mut explorer, 100, 12);
    assert_explorer_snapshot("file_explorer_with_locked_item", &output);
}

#[test]
fn snapshot_file_explorer_locked_item_preview() {
    use agr::terminal::{Cell, CellStyle, StyledLine};
    use agr::tui::widgets::SessionPreview;

    let mut explorer = FileExplorer::new(create_test_file_items_with_lock());

    // Create a preview for the locked item (first item = selected by default)
    let preview = SessionPreview {
        duration_secs: 42.0,
        marker_count: 0,
        styled_preview: vec![StyledLine {
            cells: "$ claude"
                .chars()
                .map(|c| Cell {
                    char: c,
                    style: CellStyle::default(),
                })
                .collect(),
        }],
        uncompressed_size: None,
    };

    let output = render_explorer_with_preview(&mut explorer, Some(&preview), 100, 15);
    assert_explorer_snapshot("file_explorer_locked_item_preview", &output);
}

use agr::tui::app::status_footer::{render_input_line, render_status_line};

/// Render a status bar using render_input_line and return as string.
fn render_input_line_to_string(label: &str, value: &str, width: u16) -> String {
    let height = 1u16;
    let area = Rect::new(0, 0, width, height);

    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_input_line(frame, area, label, value);
        })
        .unwrap();

    let backend = terminal.backend();
    let mut output = String::new();
    for x in 0..width {
        let cell = backend.buffer()[(x, 0)].symbol();
        output.push_str(cell);
    }
    output
}

/// Render a status bar using render_status_line and return as string.
fn render_status_line_to_string(text: &str, width: u16) -> String {
    let height = 1u16;
    let area = Rect::new(0, 0, width, height);

    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            render_status_line(frame, area, text);
        })
        .unwrap();

    let backend = terminal.backend();
    let mut output = String::new();
    for x in 0..width {
        let cell = backend.buffer()[(x, 0)].symbol();
        output.push_str(cell);
    }
    output
}

#[test]
fn snapshot_status_bar_confirm_delete() {
    let output = render_input_line_to_string("🗑  Delete? ", "(y/n) — 20250115-session.cast", 60);
    snap("status_bar_confirm_delete", &output);
}

#[test]
fn snapshot_status_bar_confirm_delete_final() {
    let output =
        render_input_line_to_string("🗑  Are you sure? ", "(y/n) — 20250115-session.cast", 60);
    snap("status_bar_confirm_delete_final", &output);
}

#[test]
fn snapshot_status_bar_confirm_unlock() {
    let output =
        render_input_line_to_string("🔓 Force unlock? ", "(y/n) — 20250115-session.cast", 60);
    snap("status_bar_confirm_unlock", &output);
}

#[test]
fn snapshot_status_bar_confirm_unlock_final() {
    let output =
        render_input_line_to_string("🔓 Are you sure? ", "(y/n) — 20250115-session.cast", 60);
    snap("status_bar_confirm_unlock_final", &output);
}

#[test]
fn snapshot_status_bar_context_menu() {
    let output = render_status_line_to_string("20250115-session.cast", 60);
    snap("status_bar_context_menu", &output);
}

#[test]
fn snapshot_status_bar_confirm_delete_long_name() {
    let output = render_input_line_to_string(
        "🗑  Delete? ",
        "(y/n) — 20250115-very-long-session-name-that-might-overflow.cast",
        80,
    );
    snap("status_bar_confirm_delete_long_name", &output);
}

#[test]
fn snapshot_status_bar_copied() {
    let output = render_status_line_to_string("🎬 Copied 20250115-session.cast to clipboard", 60);
    snap("status_bar_copied", &output);
}

// ============================================================================
// Import Modal Snapshots
// ============================================================================

#[test]
fn snapshot_import_agent_input_modal() {
    use agr::tui::import::ImportState;

    let agents = vec![
        "claude".to_string(),
        "codex".to_string(),
        "cursor".to_string(),
        "gemini".to_string(),
    ];

    let mut state = ImportState::new("/path/session1.cast\n/path/session2.cast", &agents);
    // Type "c" to filter autocomplete
    state.agent_input_char('c');
    state.update_agent_filter(&agents);

    let width = 60u16;
    let height = 20u16;
    let area = Rect::new(0, 0, width, height);

    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            agr::tui::import::render(&state, frame, area);
        })
        .unwrap();

    let backend = terminal.backend();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = backend.buffer()[(x, y)].symbol();
            output.push_str(cell);
        }
        output.push('\n');
    }
    snap("import_agent_input_modal", &output);
}

#[test]
fn snapshot_import_result_modal_success() {
    use agr::tui::import::{ImportPhase, ImportResult, ImportState};
    use std::path::PathBuf;

    let agents = vec!["claude".to_string()];
    let mut state = ImportState::new("/path/session1.cast\n/path/session2.cast", &agents);

    // Simulate successful imports
    state.phase = ImportPhase::Done;
    state.results = vec![
        ImportResult {
            filename: "session1.cast".to_string(),
            outcome: Ok(PathBuf::from("/sessions/claude/session1.cast")),
        },
        ImportResult {
            filename: "session2.cast".to_string(),
            outcome: Ok(PathBuf::from("/sessions/claude/session2.cast")),
        },
    ];

    let width = 70u16;
    let height = 20u16;
    let area = Rect::new(0, 0, width, height);

    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            agr::tui::import::render(&state, frame, area);
        })
        .unwrap();

    let backend = terminal.backend();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = backend.buffer()[(x, y)].symbol();
            output.push_str(cell);
        }
        output.push('\n');
    }
    snap("import_result_modal_success", &output);
}

#[test]
fn snapshot_import_result_modal_mixed() {
    use agr::tui::import::{ImportPhase, ImportResult, ImportState};
    use std::path::PathBuf;

    let agents = vec!["claude".to_string()];
    let mut state = ImportState::new(
        "/path/session1.cast\n/path/session2.cast\n/path/invalid.txt",
        &agents,
    );

    // Simulate mixed results
    state.phase = ImportPhase::Done;
    state.results = vec![
        ImportResult {
            filename: "session1.cast".to_string(),
            outcome: Ok(PathBuf::from("/sessions/claude/session1.cast")),
        },
        ImportResult {
            filename: "session2.cast".to_string(),
            outcome: Ok(PathBuf::from("/sessions/claude/session2.cast")),
        },
        ImportResult {
            filename: "invalid.txt".to_string(),
            outcome: Err("Wrong extension: expected .cast".to_string()),
        },
    ];

    let width = 70u16;
    let height = 20u16;
    let area = Rect::new(0, 0, width, height);

    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            agr::tui::import::render(&state, frame, area);
        })
        .unwrap();

    let backend = terminal.backend();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = backend.buffer()[(x, y)].symbol();
            output.push_str(cell);
        }
        output.push('\n');
    }
    snap("import_result_modal_mixed", &output);
}
