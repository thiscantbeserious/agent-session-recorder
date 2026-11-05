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
use crate::files::codec;
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

/// Display values of one list row: name, agent, size, age, checked, backup, locked.
type Row = (String, String, String, String, bool, bool, bool);

/// Checkbox (when shown), age and `[opt]` marker spans in front of a row.
fn row_prefix<'a>(
    checked: Option<bool>,
    time_str: &str,
    has_backup: bool,
    theme: &crate::theme::Theme,
) -> Vec<Span<'a>> {
    let dim = theme.text_secondary_style();
    let mut spans = vec![];
    if let Some(checked) = checked {
        spans.push(Span::styled(if checked { "[x] " } else { "[ ] " }, dim));
    }
    spans.push(Span::styled(format!("{:>5}: ", time_str), dim));
    if has_backup {
        spans.push(Span::styled("[opt] ", theme.accent_style()));
    }
    spans
}

/// Build one list row, with the inline rename editor when it is the selected row.
fn build_list_item<'a>(
    row: &'a Row,
    is_selected: bool,
    show_checkboxes: bool,
    rename_state: Option<(&'a str, usize, bool)>,
    theme: &crate::theme::Theme,
) -> ListItem<'a> {
    let (name, agent, size_str, time_str, is_checked, has_bak, is_locked) = row;
    let checked = show_checkboxes.then_some(*is_checked);
    let mut spans = row_prefix(checked, time_str, *has_bak, theme);
    spans.extend(match rename_state {
        Some((input, cursor, all)) if is_selected => {
            let suffix = codec::session_suffix(std::path::Path::new(name));
            build_rename_item_spans(input, cursor, all, suffix, agent, size_str, theme)
        }
        _ => build_normal_item_spans(name, agent, size_str, *is_locked, theme),
    });
    ListItem::new(Line::from(spans))
}

impl FileExplorerWidget<'_> {
    /// Collect row display data first to avoid borrow issues.
    fn rows(&self) -> Vec<Row> {
        self.explorer
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
            .collect()
    }

    /// Metadata of the selected item, cloned for the preview panel.
    fn selected_preview_data(&self) -> Option<PreviewData> {
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
    }

    /// Session preview values, cloned to avoid lifetime issues.
    fn session_data(&self) -> Option<SessionData> {
        self.session_preview.map(|p| {
            (
                p.format_duration(),
                p.marker_count,
                p.styled_preview.clone(),
                p.uncompressed_size.map(crate::storage::format_uncompressed),
            )
        })
    }
}

/// The styled sessions list around the already built rows.
fn sessions_list<'a>(items: Vec<ListItem<'a>>, theme: &crate::theme::Theme) -> List<'a> {
    let highlight = Style::default()
        .bg(theme.accent)
        .fg(ratatui::style::Color::Black)
        .add_modifier(Modifier::BOLD);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Sessions ")
        .border_style(theme.text_secondary_style());
    List::new(items)
        .block(block)
        .highlight_style(highlight)
        .highlight_symbol("> ")
}

/// List on the left, preview on the right when enabled and wide enough.
fn split_area(area: Rect, with_preview: bool) -> std::rc::Rc<[Rect]> {
    if with_preview {
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area)
    } else {
        Layout::horizontal([Constraint::Percentage(100)]).split(area)
    }
}

impl FileExplorerWidget<'_> {
    /// Render the sessions list (with inline rename editor) into `area`.
    fn render_list(&mut self, area: Rect, buf: &mut Buffer, theme: &crate::theme::Theme) {
        let selected_idx = self.explorer.selected();
        let rows = self.rows();
        let items = rows.iter().enumerate().map(|(idx, row)| {
            let rename = self.rename_state;
            build_list_item(
                row,
                idx == selected_idx,
                self.show_checkboxes,
                rename,
                theme,
            )
        });
        let list = sessions_list(items.collect(), theme);
        ratatui::widgets::StatefulWidget::render(list, area, buf, self.explorer.list_state());
    }
}

impl Widget for FileExplorerWidget<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        let theme = current_theme();
        let with_preview = self.show_preview && area.width >= 60;
        let chunks = split_area(area, with_preview);
        let preview_data = with_preview.then(|| self.selected_preview_data()).flatten();
        let (session_data, has_backup) = (self.session_data(), self.has_backup);
        self.render_list(chunks[0], buf, &theme);
        if with_preview {
            render_preview_panel(
                buf,
                chunks[1],
                preview_data,
                session_data,
                has_backup,
                &theme,
            );
        }
    }
}
