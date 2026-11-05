//! Visual snapshot tests for TUI modals (context menu, optimize result, help, unlock).

use crate::helpers::tui_snapshot::snap;
use ratatui::layout::Rect;

use agr::tui::list_app::ListApp;

/// Render the context menu modal to a buffer and return as string.
fn render_context_menu_to_string(selected_idx: usize, backup_exists: bool) -> String {
    let width = 60u16;
    let height = 18u16;
    let area = Rect::new(0, 0, width, height);

    // Create a mock terminal backend
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            ListApp::render_context_menu_modal(frame, area, selected_idx, backup_exists);
        })
        .unwrap();

    // Extract the buffer content
    let backend = terminal.backend();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = backend.buffer()[(x, y)].symbol();
            output.push_str(cell);
        }
        output.push('\n');
    }
    output
}

#[test]
fn snapshot_context_menu_first_item_selected() {
    let output = render_context_menu_to_string(0, true);
    snap("context_menu_first_item", &output);
}

#[test]
fn snapshot_context_menu_transform_selected() {
    let output = render_context_menu_to_string(3, true);
    snap("context_menu_transform_selected", &output);
}

#[test]
fn snapshot_context_menu_restore_selected_with_backup() {
    let output = render_context_menu_to_string(5, true);
    snap("context_menu_restore_with_backup", &output);
}

#[test]
fn snapshot_context_menu_restore_selected_no_backup() {
    let output = render_context_menu_to_string(5, false);
    snap("context_menu_restore_no_backup", &output);
}

#[test]
fn snapshot_context_menu_delete_selected() {
    let output = render_context_menu_to_string(6, true);
    snap("context_menu_delete_selected", &output);
}

#[test]
fn snapshot_context_menu_last_item_selected() {
    let output = render_context_menu_to_string(6, true);
    snap("context_menu_last_item", &output);
}

use agr::asciicast::TransformResult;
use agr::tui::list_app::OptimizeResultState;
use std::path::PathBuf;

/// Render the optimize result modal to a buffer and return as string.
fn render_optimize_result_to_string(result_state: &OptimizeResultState) -> String {
    let width = 60u16;
    let height = 15u16;
    let area = Rect::new(0, 0, width, height);

    // Create a mock terminal backend
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            ListApp::render_optimize_result_modal(frame, area, result_state);
        })
        .unwrap();

    // Extract the buffer content
    let backend = terminal.backend();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = backend.buffer()[(x, y)].symbol();
            output.push_str(cell);
        }
        output.push('\n');
    }
    output
}

#[test]
fn snapshot_optimize_result_success() {
    let result_state = OptimizeResultState {
        filename: "20240115-session.cast".to_string(),
        result: Ok(TransformResult {
            original_duration: 3661.5, // 1h 1m 1s
            new_duration: 1234.0,      // 20m 34s
            backup_path: Some(PathBuf::from("/tmp/test.cast.bak")),
            backup_created: true,
        }),
    };

    let output = render_optimize_result_to_string(&result_state);
    snap("optimize_result_success", &output);
}

#[test]
fn snapshot_optimize_result_success_existing_backup() {
    let result_state = OptimizeResultState {
        filename: "session.cast".to_string(),
        result: Ok(TransformResult {
            original_duration: 300.0, // 5m
            new_duration: 180.0,      // 3m
            backup_path: Some(PathBuf::from("/tmp/test.cast.bak")),
            backup_created: false, // Using existing backup
        }),
    };

    let output = render_optimize_result_to_string(&result_state);
    snap("optimize_result_existing_backup", &output);
}

#[test]
fn snapshot_optimize_result_error() {
    let result_state = OptimizeResultState {
        filename: "broken.cast".to_string(),
        result: Err("Failed to parse asciicast: invalid JSON at line 5".to_string()),
    };

    let output = render_optimize_result_to_string(&result_state);
    snap("optimize_result_error", &output);
}

/// Render the help modal to a buffer and return as string.
fn render_help_modal_to_string() -> String {
    let width = 70u16;
    let height = 30u16;
    let area = Rect::new(0, 0, width, height);

    // Create a mock terminal backend
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            ListApp::render_help_modal(frame, area);
        })
        .unwrap();

    // Extract the buffer content
    let backend = terminal.backend();
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = backend.buffer()[(x, y)].symbol();
            output.push_str(cell);
        }
        output.push('\n');
    }
    output
}

#[test]
fn snapshot_help_modal() {
    let output = render_help_modal_to_string();
    snap("help_modal", &output);
}

#[test]
fn snapshot_confirm_unlock_modal() {
    let width = 60u16;
    let height = 12u16;
    let area = Rect::new(0, 0, width, height);

    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            agr::tui::app::modals::render_confirm_unlock_modal(
                frame,
                area,
                "PID 12345 since 2025-01-15T10:30",
                false,
            );
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
    snap("confirm_unlock_modal", &output);
}
