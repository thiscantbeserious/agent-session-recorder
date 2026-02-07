//! Explorer list view rendering
//!
//! Renders the `FileExplorerWidget` into the top chunk of the layout.
//! Configures checkbox toggles and backup indicators per app context.

use ratatui::{layout::Rect, Frame};

use crate::tui::lru_cache::PreviewCache;
use crate::tui::widgets::{FileExplorer, FileExplorerWidget, SessionPreview};

/// Render the file explorer list widget into the given area.
///
/// Builds a `FileExplorerWidget` with the provided options and renders it.
///
/// Stub for now -- will be populated in Stage 5 with the widget
/// configuration logic extracted from list_app and cleanup_app draw methods.
#[allow(dead_code)]
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

/// Extract the current preview from cache for the selected item.
///
/// Returns `None` if no item is selected or preview is not yet cached.
#[allow(dead_code)]
pub fn extract_preview<'a>(
    explorer: &FileExplorer,
    cache: &'a mut PreviewCache,
) -> Option<&'a SessionPreview> {
    let path = explorer.selected_item().map(|i| i.path.clone())?;
    cache.get(&path)
}
