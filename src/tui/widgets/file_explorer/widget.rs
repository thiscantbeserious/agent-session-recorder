//! Stateless ratatui widget that renders the explorer list and preview panel.

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Widget},
};

use super::render_helpers::*;
use super::{FileExplorer, SessionPreview};
use crate::theme::current_theme;

/// Stateless widget for rendering the file explorer
pub struct FileExplorerWidget<'a> {
    explorer: &'a mut FileExplorer,
    show_preview: bool,
    show_checkboxes: bool,
    /// Optional enhanced preview data for the selected session
    session_preview: Option<&'a SessionPreview>,
    /// Whether a backup exists for the selected file
    has_backup: bool,
    /// When set, shows inline rename input on the selected item: (text, cursor_pos, selected_all)
    rename_state: Option<(&'a str, usize, bool)>,
}

impl<'a> FileExplorerWidget<'a> {
    /// Create a new file explorer widget
    pub fn new(explorer: &'a mut FileExplorer) -> Self {
        Self {
            explorer,
            show_preview: true,
            show_checkboxes: true,
            session_preview: None,
            has_backup: false,
            rename_state: None,
        }
    }

    /// Show or hide the preview panel
    pub fn show_preview(mut self, show: bool) -> Self {
        self.show_preview = show;
        self
    }

    /// Show or hide checkboxes for multi-select
    pub fn show_checkboxes(mut self, show: bool) -> Self {
        self.show_checkboxes = show;
        self
    }

    /// Set the session preview data to display enhanced information
    pub fn session_preview(mut self, preview: Option<&'a SessionPreview>) -> Self {
        self.session_preview = preview;
        self
    }

    /// Set whether a backup exists for the selected file
    pub fn has_backup(mut self, has_backup: bool) -> Self {
        self.has_backup = has_backup;
        self
    }

    /// Set inline rename state: (text, cursor_pos, selected_all)
    pub fn rename_state(mut self, state: Option<(&'a str, usize, bool)>) -> Self {
        self.rename_state = state;
        self
    }
}

impl Widget for FileExplorerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = current_theme();

        // Layout: list on left, preview on right (if enabled)
        let chunks = if self.show_preview && area.width >= 60 {
            Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area)
        } else {
            Layout::horizontal([Constraint::Percentage(100)]).split(area)
        };

        // Build list items (collect data first to avoid borrow issues)
        // Note: has_backup and lock_info are cached in FileItem
        let item_data: Vec<(String, String, String, String, bool, bool, bool)> = self
            .explorer
            .visible_items()
            .map(|(_, item, is_checked)| {
                (
                    item.name.clone(),
                    item.agent.clone(),
                    format_size(item.size),
                    crate::storage::format_smart_time(&item.modified),
                    is_checked,
                    item.has_backup,
                    item.lock_info.is_some(),
                )
            })
            .collect();

        let show_checkboxes = self.show_checkboxes;
        let selected_idx = self.explorer.selected();
        let rename_state = self.rename_state;
        let items: Vec<ListItem> = item_data
            .iter()
            .enumerate()
            .map(
                |(idx, (name, agent, size_str, time_str, is_checked, has_bak, is_locked))| {
                    let mut spans = vec![];
                    if show_checkboxes {
                        let checkbox = if *is_checked { "[x] " } else { "[ ] " };
                        spans.push(Span::styled(checkbox, theme.text_secondary_style()));
                    }
                    spans.push(Span::styled(
                        format!("{:>5}: ", time_str),
                        theme.text_secondary_style(),
                    ));
                    if *has_bak {
                        spans.push(Span::styled("[opt] ", theme.accent_style()));
                    }

                    if idx == selected_idx {
                        if let Some((input, cursor, selected_all)) = rename_state {
                            let mut rename_spans = build_rename_item_spans(
                                input,
                                cursor,
                                selected_all,
                                crate::files::codec::session_suffix(std::path::Path::new(name)),
                                agent,
                                size_str,
                                &theme,
                            );
                            spans.append(&mut rename_spans);
                            return ListItem::new(Line::from(spans));
                        }
                    }

                    let mut normal_spans =
                        build_normal_item_spans(name, agent, size_str, *is_locked, &theme);
                    spans.append(&mut normal_spans);
                    ListItem::new(Line::from(spans))
                },
            )
            .collect();

        // Get preview data before mutable borrow
        let preview_data = if self.show_preview && chunks.len() > 1 {
            self.explorer.selected_item().map(|item| {
                (
                    item.name.clone(),
                    item.agent.clone(),
                    item.size,
                    item.modified,
                    item.path.clone(),
                    item.lock_info.clone(),
                )
            })
        } else {
            None
        };

        // Clone session preview data to avoid lifetime issues
        let session_preview_data = self.session_preview.map(|p| {
            (
                p.format_duration(),
                p.marker_count,
                p.styled_preview.clone(),
                p.uncompressed_size.map(crate::storage::format_uncompressed),
            )
        });

        // Capture backup status
        let has_backup = self.has_backup;

        // Render list
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Sessions ")
                    .border_style(theme.text_secondary_style()),
            )
            .highlight_style(
                Style::default()
                    .bg(theme.accent)
                    .fg(ratatui::style::Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        // Render with state (mutable borrow here)
        ratatui::widgets::StatefulWidget::render(list, chunks[0], buf, self.explorer.list_state());

        // Render preview panel if enabled
        if self.show_preview && chunks.len() > 1 {
            render_preview_panel(
                buf,
                chunks[1],
                preview_data,
                session_preview_data,
                has_backup,
                &theme,
            );
        }
    }
}
