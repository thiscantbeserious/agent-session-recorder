//! Explorer list view rendering
//!
//! Renders the `FileExplorerWidget` into the top chunk of the layout.
//! Configures checkbox toggles and backup indicators per app context.

use ratatui::{layout::Rect, Frame};

use crate::tui::widgets::{FileExplorer, FileExplorerWidget, SessionPreview};

/// Render the file explorer list widget into the given area.
///
/// Builds a `FileExplorerWidget` with the provided options and renders it.
/// Both `list_app` and `cleanup_app` use this with different flag combinations:
/// - `show_checkboxes`: true for cleanup (multi-select), false for list
/// - `has_backup`: true when the selected file has a backup (list only)
pub fn render_explorer_list(
    frame: &mut Frame,
    area: Rect,
    explorer: &mut FileExplorer,
    preview: Option<&SessionPreview>,
    show_checkboxes: bool,
    has_backup: bool,
) {
    let widget = FileExplorerWidget::new(explorer)
        .show_checkboxes(show_checkboxes)
        .session_preview(preview)
        .has_backup(has_backup);
    frame.render_widget(widget, area);
}
