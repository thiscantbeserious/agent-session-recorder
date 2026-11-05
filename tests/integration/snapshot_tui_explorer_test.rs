//! Visual snapshot tests for the file explorer widget.

use crate::helpers::tui_snapshot::{
    assert_explorer_snapshot, render_explorer_to_string, render_explorer_with_preview,
    render_explorer_with_preview_and_backup,
};
use agr::tui::widgets::{FileExplorer, FileItem};
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

/// Snapshot a file explorer render with the relative age column (e.g. `2y1m`) masked.
///
/// The age is computed against `Local::now()` while fixtures use fixed dates,

#[test]
fn snapshot_file_explorer_basic() {
    let mut explorer = FileExplorer::new(create_test_file_items());

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    assert_explorer_snapshot("file_explorer_basic", &output);
}

#[test]
fn snapshot_file_explorer_with_selection() {
    let mut explorer = FileExplorer::new(create_test_file_items());
    // Move to second item
    explorer.down();

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    assert_explorer_snapshot("file_explorer_with_selection", &output);
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
    assert_explorer_snapshot("file_explorer_multi_select", &output);
}

#[test]
fn snapshot_file_explorer_filtered_by_agent() {
    let mut explorer = FileExplorer::new(create_test_file_items());
    explorer.set_agent_filter(Some("claude".to_string()));

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    assert_explorer_snapshot("file_explorer_filtered", &output);
}

#[test]
fn snapshot_file_explorer_sorted_by_name() {
    use agr::tui::widgets::SortField;

    let mut explorer = FileExplorer::new(create_test_file_items());
    explorer.set_sort(SortField::Name);

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    assert_explorer_snapshot("file_explorer_sorted_name", &output);
}

#[test]
fn snapshot_file_explorer_sorted_by_size() {
    use agr::tui::widgets::SortField;

    let mut explorer = FileExplorer::new(create_test_file_items());
    explorer.set_sort(SortField::Size);

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    assert_explorer_snapshot("file_explorer_sorted_size", &output);
}

#[test]
fn snapshot_file_explorer_narrow() {
    let mut explorer = FileExplorer::new(create_test_file_items());

    // Narrow width - should hide preview panel
    let output = render_explorer_to_string(&mut explorer, 50, 15);
    assert_explorer_snapshot("file_explorer_narrow", &output);
}

#[test]
fn snapshot_file_explorer_empty() {
    let mut explorer = FileExplorer::new(vec![]);

    let output = render_explorer_to_string(&mut explorer, 100, 15);
    assert_explorer_snapshot("file_explorer_empty", &output);
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

#[test]
fn snapshot_file_explorer_with_session_preview() {
    use agr::terminal::{Cell, CellStyle, Color, StyledLine};
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
        uncompressed_size: None,
    };

    let output = render_explorer_with_preview(&mut explorer, Some(&preview), 100, 20);
    assert_explorer_snapshot("file_explorer_with_session_preview", &output);
}

#[test]
fn snapshot_file_explorer_preview_with_backup() {
    use agr::terminal::{Cell, CellStyle, StyledLine};
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
        uncompressed_size: None,
    };

    // Render with backup indicator
    let output =
        render_explorer_with_preview_and_backup(&mut explorer, Some(&preview), true, 100, 20);
    assert_explorer_snapshot("file_explorer_preview_with_backup", &output);
}

#[test]
fn snapshot_file_explorer_preview_without_backup() {
    use agr::terminal::{Cell, CellStyle, StyledLine};
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
        uncompressed_size: None,
    };

    // Render without backup indicator
    let output =
        render_explorer_with_preview_and_backup(&mut explorer, Some(&preview), false, 100, 20);
    assert_explorer_snapshot("file_explorer_preview_without_backup", &output);
}

#[test]
fn snapshot_file_explorer_preview_gzip_sizes() {
    use agr::files::codec::UncompressedSize;
    use agr::tui::widgets::SessionPreview;

    let mut explorer = FileExplorer::new(vec![FileItem::new(
        "/sessions/claude/20240115-session1.cast.gz",
        "20240115-session1.cast.gz",
        "claude",
        1024 * 5, // 5 KB on disk
        Local.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap(),
    )]);
    let preview = SessionPreview {
        duration_secs: 60.0,
        marker_count: 1,
        styled_preview: Vec::new(),
        uncompressed_size: Some(UncompressedSize::Exact(1024 * 50)),
    };

    let output = render_explorer_with_preview(&mut explorer, Some(&preview), 100, 12);
    assert!(output.contains("Uncompressed: 50 KiB"), "{output}");
    assert_explorer_snapshot("file_explorer_preview_gzip_sizes", &output);
}
