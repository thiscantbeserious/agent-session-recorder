//! Shared helpers for the TUI snapshot tests.
//!
//! The snapshot files keep the historic `integration__snapshot_tui_test__` prefix even
//! though the tests live in several modules, as such the full name is passed explicitly
//! and the module path is not prepended.

use agr::tui::widgets::{FileExplorer, FileExplorerWidget};
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

/// Assert a snapshot under the historic `snapshot_tui_test` file name.
pub fn snap(name: &str, output: &str) {
    insta::with_settings!({prepend_module_to_snapshot => false,
        snapshot_path => "../snapshots"}, {
        insta::assert_snapshot!(format!("integration__snapshot_tui_test__{name}"), output);
    });
}

/// Snapshot a file explorer render with the relative age column (e.g. `2y1m`) masked.
///
/// The age is computed against `Local::now()` while fixtures use fixed dates,
/// as such it changes every month. Replacement keeps the column width.
pub fn assert_explorer_snapshot(name: &str, output: &str) {
    insta::with_settings!({
        prepend_module_to_snapshot => false,
        snapshot_path => "../snapshots",
        filters => vec![(r"\s+\d+y\d+m:", "  #y#m:")]
    }, {
        insta::assert_snapshot!(format!("integration__snapshot_tui_test__{name}"), output);
    });
}

/// Render a file explorer widget to a string for snapshot testing.
pub fn render_explorer_to_string(explorer: &mut FileExplorer, width: u16, height: u16) -> String {
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

/// Render a file explorer widget with a session preview to a string.
pub fn render_explorer_with_preview(
    explorer: &mut FileExplorer,
    preview: Option<&agr::tui::widgets::SessionPreview>,
    width: u16,
    height: u16,
) -> String {
    render_explorer_with_preview_and_backup(explorer, preview, false, width, height)
}

/// Render a file explorer widget with a session preview and backup indicator.
pub fn render_explorer_with_preview_and_backup(
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
