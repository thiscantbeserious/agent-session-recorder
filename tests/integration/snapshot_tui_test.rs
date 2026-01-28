//! Visual snapshot tests for TUI components
//!
//! Uses insta for snapshot testing to ensure visual output is correct.

use agr::tui::theme::current_theme;
use agr::tui::widgets::Logo;
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

/// Render a widget to a string for snapshot testing.
fn render_to_string<W: Widget>(widget: W, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    // Convert buffer to string representation with ANSI colors
    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            // Include fg color info in a readable format
            let symbol = cell.symbol();
            output.push_str(symbol);
        }
        output.push('\n');
    }
    output
}

/// Render a widget to a detailed string with color information.
fn render_with_colors<W: Widget>(widget: W, width: u16, height: u16) -> String {
    use ratatui::style::Color;

    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    widget.render(area, &mut buf);

    let mut output = String::new();

    // Header with dimensions
    output.push_str(&format!("=== {}x{} ===\n", width, height));

    // Track unique colors used
    let mut colors_used: Vec<(String, Color)> = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            if let Some(fg) = cell.style().fg {
                if !colors_used.iter().any(|(_, c)| *c == fg) {
                    let label = match fg {
                        Color::Rgb(r, g, b) => format!("RGB({},{},{})", r, g, b),
                        other => format!("{:?}", other),
                    };
                    colors_used.push((label, fg));
                }
            }
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }

    // Color summary
    if !colors_used.is_empty() {
        output.push_str("\n--- Colors Used ---\n");
        for (label, _) in colors_used {
            output.push_str(&format!("  {}\n", label));
        }
    }

    output
}

#[test]
fn snapshot_logo_visual() {
    let logo = Logo::new();
    let output = render_with_colors(logo, 80, 12);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_theme_colors() {
    let theme = current_theme();
    let snapshot = format!(
        "Theme: claude_code\n\
         text_primary: {:?}\n\
         text_secondary: {:?}\n\
         accent: {:?}\n\
         error: {:?}\n\
         success: {:?}\n\
         background: {:?}",
        theme.text_primary,
        theme.text_secondary,
        theme.accent,
        theme.error,
        theme.success,
        theme.background
    );
    insta::assert_snapshot!(snapshot);
}

#[test]
fn snapshot_logo_rec_line_scales() {
    // Test that the REC line scales to different widths
    let logo_40 = Logo::new();
    let logo_80 = Logo::new();
    let logo_120 = Logo::new();

    let out_40 = render_to_string(logo_40, 40, 12);
    let out_80 = render_to_string(logo_80, 80, 12);
    let out_120 = render_to_string(logo_120, 120, 12);

    let snapshot = format!(
        "=== Width 40 ===\n{}\n\
         === Width 80 ===\n{}\n\
         === Width 120 ===\n{}",
        out_40, out_80, out_120
    );
    insta::assert_snapshot!(snapshot);
}

#[test]
fn logo_uses_accent_color_for_logo_text() {
    use ratatui::style::Color;

    let logo = Logo::new();

    let area = Rect::new(0, 0, 80, 12);
    let mut buf = Buffer::empty(area);
    logo.render(area, &mut buf);

    // Check a character from the logo (row 2, should be part of 'A')
    // The logo starts after 2 blank lines, so row 2 is the first logo line
    let logo_cell = &buf[(0, 2)]; // First char of first logo line

    // Logo should use accent color (bright green)
    assert_eq!(
        logo_cell.style().fg,
        Some(Color::LightGreen),
        "Logo should use accent color (bright green)"
    );
}

#[test]
fn logo_uses_secondary_color_for_dashes() {
    use ratatui::style::Color;

    let logo = Logo::new();

    let area = Rect::new(0, 0, 80, 12);
    let mut buf = Buffer::empty(area);
    logo.render(area, &mut buf);

    // REC line is at row 8 (2 blank + 6 logo lines)
    // The dashes start after "⏺ REC " (7 chars)
    let dash_cell = &buf[(10, 8)]; // A dash character

    // Dashes should use text_secondary color (dark gray)
    assert_eq!(
        dash_cell.style().fg,
        Some(Color::DarkGray),
        "Dashes should use text_secondary color (dark gray)"
    );
}

#[test]
fn snapshot_cli_ansi_theme_colors() {
    // Snapshot the ANSI color codes used by the theme for CLI output
    let theme = current_theme();

    let snapshot = format!(
        "CLI ANSI Theme Colors\n\
         =====================\n\
         accent_text(\"test\"): {:?}\n\
         primary_text(\"test\"): {:?}\n\
         secondary_text(\"test\"): {:?}\n\
         error_text(\"test\"): {:?}\n\
         success_text(\"test\"): {:?}",
        theme.accent_text("test"),
        theme.primary_text("test"),
        theme.secondary_text("test"),
        theme.error_text("test"),
        theme.success_text("test"),
    );
    insta::assert_snapshot!(snapshot);
}

// ============================================================================
// Branding Assets Snapshots
// ============================================================================

#[test]
fn snapshot_branding_logo_start() {
    // Small logo shown when starting a recording session
    let logo = agr::branding::LOGO_START;
    insta::assert_snapshot!("branding_logo_start", logo);
}

#[test]
fn snapshot_branding_logo_done() {
    // Small logo shown when a recording session ends
    let logo = agr::branding::LOGO_DONE;
    insta::assert_snapshot!("branding_logo_done", logo);
}

#[test]
fn snapshot_branding_logo_full() {
    // Full ASCII logo for interactive CLI mode
    let logo = agr::branding::LOGO_FULL;
    insta::assert_snapshot!("branding_logo_full", logo);
}

#[test]
fn snapshot_branding_box_bottom() {
    // Bottom border of the box
    let border = agr::branding::BOX_BOTTOM;
    insta::assert_snapshot!("branding_box_bottom", border);
}

// ============================================================================
// Colorize Help Function Snapshots
// ============================================================================

#[test]
fn snapshot_colorize_help_commands() {
    use agr::tui::colorize_help;

    // Test colorize_help with typical command list output
    let input = r#"
Commands:
  record   Start recording a session
  status   Show storage statistics
  cleanup  Interactive cleanup of old sessions
  list     List recorded sessions [aliases: ls]
  analyze  Analyze a recording with AI
"#;

    let output = colorize_help(input);
    insta::assert_snapshot!("colorize_help_commands", output);
}

#[test]
fn snapshot_colorize_help_logo() {
    use agr::tui::colorize_help;

    // Test colorize_help with logo (should apply green to logo lines)
    let input = r#" █████╗  ██████╗ ██████╗
██╔══██╗██╔════╝ ██╔══██╗
███████║██║  ███╗██████╔╝
██╔══██║██║   ██║██╔══██╗
██║  ██║╚██████╔╝██║  ██║
╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
 ⏺ REC ─────────────────────────────────────────────────────────────────────────
"#;

    let output = colorize_help(input);
    insta::assert_snapshot!("colorize_help_logo", output);
}

#[test]
fn snapshot_colorize_help_mixed() {
    use agr::tui::colorize_help;

    // Test colorize_help with mixed content (logo + commands)
    let input = r#" █████╗  ██████╗ ██████╗
██╔══██╗██╔════╝ ██╔══██╗
███████║██║  ███╗██████╔╝
██╔══██║██║   ██║██╔══██╗
██║  ██║╚██████╔╝██║  ██║
╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
 ⏺ REC ─────────────────────────────────────────────────────────────────────────


[ Agent Session Recorder ] - auto-record agent sessions!

Usage: agr <COMMAND>

Commands:
  record   Start recording a session
  status   Show storage statistics
"#;

    let output = colorize_help(input);
    insta::assert_snapshot!("colorize_help_mixed", output);
}

// ============================================================================
// Static Logo Builder Snapshots (Fixed Width)
// ============================================================================

#[test]
fn snapshot_static_logo_width_40() {
    use agr::tui::widgets::logo::build_static_logo;
    let output = build_static_logo(40);
    insta::assert_snapshot!("static_logo_width_40", output);
}

#[test]
fn snapshot_static_logo_width_80() {
    use agr::tui::widgets::logo::build_static_logo;
    let output = build_static_logo(80);
    insta::assert_snapshot!("static_logo_width_80", output);
}

#[test]
fn snapshot_static_logo_width_120() {
    use agr::tui::widgets::logo::build_static_logo;
    let output = build_static_logo(120);
    insta::assert_snapshot!("static_logo_width_120", output);
}

// ============================================================================
// Theme Text Wrapper Snapshots
// ============================================================================

#[test]
fn snapshot_theme_text_wrappers() {
    let theme = current_theme();

    let snapshot = format!(
        "Theme Text Wrappers (Raw ANSI codes)\n\
         ====================================\n\
         \n\
         accent_text(\"Hello World\"):\n{}\n\
         \n\
         primary_text(\"Hello World\"):\n{}\n\
         \n\
         secondary_text(\"Hello World\"):\n{}\n\
         \n\
         error_text(\"Hello World\"):\n{}\n\
         \n\
         success_text(\"Hello World\"):\n{}",
        theme.accent_text("Hello World"),
        theme.primary_text("Hello World"),
        theme.secondary_text("Hello World"),
        theme.error_text("Hello World"),
        theme.success_text("Hello World"),
    );
    insta::assert_snapshot!(snapshot);
}

// ============================================================================
// File Explorer Widget Snapshots
// ============================================================================

use agr::tui::widgets::{FileExplorer, FileExplorerWidget, FileItem};
use chrono::{Local, TimeZone};

fn create_test_file_items() -> Vec<FileItem> {
    vec![
        FileItem::new(
            "/sessions/claude/20240115-session1.cast",
            "20240115-session1.cast",
            "claude",
            1024 * 50, // 50 KB
            Local.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap(),
        ),
        FileItem::new(
            "/sessions/codex/20240116-session2.cast",
            "20240116-session2.cast",
            "codex",
            1024 * 1024 * 2, // 2 MB
            Local.with_ymd_and_hms(2024, 1, 16, 14, 45, 0).unwrap(),
        ),
        FileItem::new(
            "/sessions/claude/20240114-session3.cast",
            "20240114-session3.cast",
            "claude",
            1024 * 100, // 100 KB
            Local.with_ymd_and_hms(2024, 1, 14, 9, 0, 0).unwrap(),
        ),
        FileItem::new(
            "/sessions/gemini/20240117-session4.cast",
            "20240117-session4.cast",
            "gemini",
            1024 * 1024, // 1 MB
            Local.with_ymd_and_hms(2024, 1, 17, 16, 0, 0).unwrap(),
        ),
    ]
}

/// Render a file explorer widget to a string for snapshot testing.
fn render_explorer_to_string(explorer: &mut FileExplorer, width: u16, height: u16) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);

    let widget = FileExplorerWidget::new(explorer);
    widget.render(area, &mut buf);

    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn snapshot_file_explorer_basic() {
    let mut explorer = FileExplorer::new(create_test_file_items());

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    insta::assert_snapshot!("file_explorer_basic", output);
}

#[test]
fn snapshot_file_explorer_with_selection() {
    let mut explorer = FileExplorer::new(create_test_file_items());
    // Move to second item
    explorer.down();

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    insta::assert_snapshot!("file_explorer_with_selection", output);
}

#[test]
fn snapshot_file_explorer_with_multi_select() {
    let mut explorer = FileExplorer::new(create_test_file_items());
    // Select first item
    explorer.toggle_select();
    // Move to second and select
    explorer.down();
    explorer.toggle_select();
    // Move to third (not selected)
    explorer.down();

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    insta::assert_snapshot!("file_explorer_multi_select", output);
}

#[test]
fn snapshot_file_explorer_filtered_by_agent() {
    let mut explorer = FileExplorer::new(create_test_file_items());
    explorer.set_agent_filter(Some("claude".to_string()));

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    insta::assert_snapshot!("file_explorer_filtered", output);
}

#[test]
fn snapshot_file_explorer_sorted_by_name() {
    use agr::tui::widgets::SortField;

    let mut explorer = FileExplorer::new(create_test_file_items());
    explorer.set_sort(SortField::Name);

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    insta::assert_snapshot!("file_explorer_sorted_name", output);
}

#[test]
fn snapshot_file_explorer_sorted_by_size() {
    use agr::tui::widgets::SortField;

    let mut explorer = FileExplorer::new(create_test_file_items());
    explorer.set_sort(SortField::Size);

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    insta::assert_snapshot!("file_explorer_sorted_size", output);
}

#[test]
fn snapshot_file_explorer_narrow() {
    let mut explorer = FileExplorer::new(create_test_file_items());

    // Narrow width - should hide preview panel
    let output = render_explorer_to_string(&mut explorer, 50, 15);
    insta::assert_snapshot!("file_explorer_narrow", output);
}

#[test]
fn snapshot_file_explorer_empty() {
    let mut explorer = FileExplorer::new(vec![]);

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    insta::assert_snapshot!("file_explorer_empty", output);
}

#[test]
fn test_file_explorer_remove_item() {
    let mut explorer = FileExplorer::new(create_test_file_items());

    // Initially should have 4 items
    assert_eq!(explorer.len(), 4);

    // Remove an existing item
    let removed = explorer.remove_item("/sessions/codex/20240116-session2.cast");
    assert!(removed, "Should return true when item exists");
    assert_eq!(explorer.len(), 3);

    // Try to remove non-existent item
    let not_removed = explorer.remove_item("/sessions/nonexistent.cast");
    assert!(!not_removed, "Should return false when item doesn't exist");
    assert_eq!(explorer.len(), 3);
}

#[test]
fn test_file_explorer_remove_item_adjusts_selection() {
    let mut explorer = FileExplorer::new(create_test_file_items());

    // Move to the last item (index 3)
    explorer.down();
    explorer.down();
    explorer.down();

    // Remove the last item - selection should adjust
    explorer.remove_item("/sessions/gemini/20240117-session4.cast");

    // Selection should now be at the new last item (index 2)
    let selected = explorer.selected_item();
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().agent, "claude"); // Third item was claude
}

#[test]
fn test_file_explorer_remove_item_clears_multi_select() {
    let mut explorer = FileExplorer::new(create_test_file_items());

    // Multi-select the first two visible items
    // Default sort is by date descending, so order is:
    // 0: gemini/session4 (2024-01-17) - raw idx 3
    // 1: codex/session2 (2024-01-16) - raw idx 1
    // 2: claude/session1 (2024-01-15) - raw idx 0
    // 3: claude/session3 (2024-01-14) - raw idx 2
    explorer.toggle_select(); // Select gemini (first visible)
    explorer.down();
    explorer.toggle_select(); // Select codex (second visible)

    assert_eq!(explorer.selected_items().len(), 2);

    // Remove the gemini item (first selected)
    explorer.remove_item("/sessions/gemini/20240117-session4.cast");

    // Should only have one selected now (codex)
    assert_eq!(explorer.selected_items().len(), 1);
}

/// Render a file explorer widget with a session preview to a string.
fn render_explorer_with_preview(
    explorer: &mut FileExplorer,
    preview: Option<&agr::tui::widgets::SessionPreview>,
    width: u16,
    height: u16,
) -> String {
    render_explorer_with_preview_and_backup(explorer, preview, false, width, height)
}

/// Render a file explorer widget with a session preview and backup indicator.
fn render_explorer_with_preview_and_backup(
    explorer: &mut FileExplorer,
    preview: Option<&agr::tui::widgets::SessionPreview>,
    has_backup: bool,
    width: u16,
    height: u16,
) -> String {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);

    let widget = FileExplorerWidget::new(explorer)
        .session_preview(preview)
        .has_backup(has_backup);
    widget.render(area, &mut buf);

    let mut output = String::new();
    for y in 0..height {
        for x in 0..width {
            let cell = &buf[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn snapshot_file_explorer_with_session_preview() {
    use agr::terminal_buffer::{Cell, CellStyle, Color, StyledLine};
    use agr::tui::widgets::SessionPreview;

    let mut explorer = FileExplorer::new(create_test_file_items());

    // Create a mock session preview
    let preview = SessionPreview {
        duration_secs: 125.5, // 2m 5s
        marker_count: 3,
        styled_preview: vec![
            StyledLine {
                cells: "$ cargo build"
                    .chars()
                    .map(|c| Cell {
                        char: c,
                        style: CellStyle::default(),
                    })
                    .collect(),
            },
            StyledLine {
                cells: "   Compiling agr v0.1.0"
                    .chars()
                    .enumerate()
                    .map(|(i, c)| Cell {
                        char: c,
                        style: if i >= 3 {
                            CellStyle {
                                fg: Color::Green,
                                ..CellStyle::default()
                            }
                        } else {
                            CellStyle::default()
                        },
                    })
                    .collect(),
            },
        ],
    };

    let output = render_explorer_with_preview(&mut explorer, Some(&preview), 100, 20);
    insta::assert_snapshot!("file_explorer_with_session_preview", output);
}

#[test]
fn snapshot_file_explorer_preview_with_backup() {
    use agr::terminal_buffer::{Cell, CellStyle, StyledLine};
    use agr::tui::widgets::SessionPreview;

    let mut explorer = FileExplorer::new(create_test_file_items());

    // Create a mock session preview
    let preview = SessionPreview {
        duration_secs: 300.0, // 5m 0s
        marker_count: 2,
        styled_preview: vec![StyledLine {
            cells: "$ echo hello"
                .chars()
                .map(|c| Cell {
                    char: c,
                    style: CellStyle::default(),
                })
                .collect(),
        }],
    };

    // Render with backup indicator
    let output =
        render_explorer_with_preview_and_backup(&mut explorer, Some(&preview), true, 100, 20);
    insta::assert_snapshot!("file_explorer_preview_with_backup", output);
}

#[test]
fn snapshot_file_explorer_preview_without_backup() {
    use agr::terminal_buffer::{Cell, CellStyle, StyledLine};
    use agr::tui::widgets::SessionPreview;

    let mut explorer = FileExplorer::new(create_test_file_items());

    // Create a mock session preview
    let preview = SessionPreview {
        duration_secs: 300.0, // 5m 0s
        marker_count: 2,
        styled_preview: vec![StyledLine {
            cells: "$ echo hello"
                .chars()
                .map(|c| Cell {
                    char: c,
                    style: CellStyle::default(),
                })
                .collect(),
        }],
    };

    // Render without backup indicator
    let output =
        render_explorer_with_preview_and_backup(&mut explorer, Some(&preview), false, 100, 20);
    insta::assert_snapshot!("file_explorer_preview_without_backup", output);
}

// ============================================================================
// Context Menu Modal Snapshots
// ============================================================================

use agr::tui::list_app::ListApp;

/// Render the context menu modal to a buffer and return as string.
fn render_context_menu_to_string(selected_idx: usize, backup_exists: bool) -> String {
    let width = 60u16;
    let height = 15u16;
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
    insta::assert_snapshot!("context_menu_first_item", output);
}

#[test]
fn snapshot_context_menu_transform_selected() {
    let output = render_context_menu_to_string(1, true);
    insta::assert_snapshot!("context_menu_transform_selected", output);
}

#[test]
fn snapshot_context_menu_restore_selected_with_backup() {
    let output = render_context_menu_to_string(2, true);
    insta::assert_snapshot!("context_menu_restore_with_backup", output);
}

#[test]
fn snapshot_context_menu_restore_selected_no_backup() {
    let output = render_context_menu_to_string(2, false);
    insta::assert_snapshot!("context_menu_restore_no_backup", output);
}

#[test]
fn snapshot_context_menu_delete_selected() {
    let output = render_context_menu_to_string(3, true);
    insta::assert_snapshot!("context_menu_delete_selected", output);
}

#[test]
fn snapshot_context_menu_last_item_selected() {
    let output = render_context_menu_to_string(4, true);
    insta::assert_snapshot!("context_menu_last_item", output);
}

// ============================================================================
// Optimize Result Modal Snapshots
// ============================================================================

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
    insta::assert_snapshot!("optimize_result_success", output);
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
    insta::assert_snapshot!("optimize_result_existing_backup", output);
}

#[test]
fn snapshot_optimize_result_error() {
    let result_state = OptimizeResultState {
        filename: "broken.cast".to_string(),
        result: Err("Failed to parse asciicast: invalid JSON at line 5".to_string()),
    };

    let output = render_optimize_result_to_string(&result_state);
    insta::assert_snapshot!("optimize_result_error", output);
}

// ============================================================================
// Help Modal Snapshots
// ============================================================================

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
    insta::assert_snapshot!("help_modal", output);
}
