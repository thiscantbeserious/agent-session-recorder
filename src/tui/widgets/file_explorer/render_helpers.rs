//! Free rendering helpers for the explorer widget: item spans, preview panel, size format.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use super::SessionPreview;
use crate::files::lock::LockInfo;

/// Pre-collected metadata for the preview panel to avoid borrow issues during rendering.
pub(super) type PreviewData = (
    String,
    String,
    u64,
    chrono::DateTime<chrono::Local>,
    String,
    Option<LockInfo>,
);

/// Session preview values for the panel: duration, markers, terminal snapshot and the
/// formatted uncompressed size (only for compressed sessions).
pub(super) type SessionData = (
    String,
    usize,
    Vec<crate::terminal::StyledLine>,
    Option<String>,
);

/// Inputs of the inline rename row, `cursor` is a byte offset into `input`.
pub(super) struct RenameRow<'a> {
    pub input: &'a str,
    pub cursor: usize,
    pub selected_all: bool,
    pub suffix: &'a str,
    pub agent: &'a str,
    pub size_str: &'a str,
}

/// Build spans for an item that is currently being renamed.
///
/// Handles both `selected_all` mode (entire text highlighted) and normal edit mode
/// with a block cursor rendered at the byte position `cursor` within `input`.
/// `suffix` is the real session suffix (`.cast` or `.cast.gz`).
///
/// `cursor` is clamped to a valid UTF-8 boundary to prevent panics on caller error.
pub(super) fn build_rename_item_spans<'a>(
    row: &RenameRow<'a>,
    theme: &crate::theme::Theme,
) -> Vec<Span<'a>> {
    let (input, suffix) = (row.input, row.suffix);
    let cursor = clamp_to_boundary(input, row.cursor);
    let mut spans = if row.selected_all {
        let style = theme.highlight_style().add_modifier(Modifier::SLOW_BLINK);
        vec![Span::styled(input, style)]
    } else {
        edit_spans(input, cursor, suffix, theme)
    };
    if row.selected_all || cursor < input.len() {
        spans.push(Span::styled(suffix, theme.text_secondary_style()));
    }
    spans.extend(meta_spans(row.agent, row.size_str, theme));
    spans
}

/// Two spaces then the dimmed `(agent, size)` tail of a list row.
fn meta_spans<'a>(agent: &str, size_str: &str, theme: &crate::theme::Theme) -> [Span<'a>; 2] {
    [
        Span::raw("  "),
        Span::styled(
            format!("({}, {})", agent, size_str),
            theme.text_secondary_style(),
        ),
    ]
}

/// Clamp `cursor` to `input` and snap it back to a UTF-8 boundary.
fn clamp_to_boundary(input: &str, cursor: usize) -> usize {
    let cursor = cursor.min(input.len());
    if input.is_char_boundary(cursor) {
        return cursor;
    }
    input
        .char_indices()
        .map(|(idx, _)| idx)
        .take_while(|&idx| idx < cursor)
        .last()
        .unwrap_or(0)
}

/// Spans for edit mode: text before, cursor block, text after.
///
/// With the cursor at the end the first suffix char (the dot) is the cursor block and
/// the rest of the suffix follows, otherwise the caller appends the whole suffix.
fn edit_spans<'a>(
    input: &'a str,
    cursor: usize,
    suffix: &'a str,
    theme: &crate::theme::Theme,
) -> Vec<Span<'a>> {
    let cursor_style = theme.highlight_style().add_modifier(Modifier::SLOW_BLINK);
    let mut spans = vec![];
    push_text(&mut spans, &input[..cursor], theme);
    let Some(c) = input[cursor..].chars().next() else {
        spans.push(Span::styled(&suffix[..1], cursor_style));
        spans.push(Span::styled(&suffix[1..], theme.text_secondary_style()));
        return spans;
    };
    let end = cursor + c.len_utf8();
    spans.push(Span::styled(&input[cursor..end], cursor_style));
    push_text(&mut spans, &input[end..], theme);
    spans
}

fn push_text<'a>(spans: &mut Vec<Span<'a>>, text: &'a str, theme: &crate::theme::Theme) {
    if !text.is_empty() {
        spans.push(Span::styled(text, theme.text_style()));
    }
}

/// Build spans for a normal (non-rename) list item.
///
/// Applies a dimmed style to locked (actively recording) items and appends
/// the recording indicator emoji for locked files.
pub(super) fn build_normal_item_spans<'a>(
    name: &'a str,
    agent: &str,
    size_str: &str,
    is_locked: bool,
    theme: &crate::theme::Theme,
) -> Vec<Span<'a>> {
    let name_style = if is_locked {
        theme.text_secondary_style()
    } else {
        theme.text_style()
    };

    let mut spans: Vec<Span<'a>> = vec![Span::styled(name, name_style)];

    if is_locked {
        spans.push(Span::styled(" \u{1F4F9}", theme.text_secondary_style()));
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("({}, {})", agent, size_str),
        theme.text_secondary_style(),
    ));
    spans
}

/// Render the right-hand preview panel for the currently selected session.
///
/// Displays file metadata, optional session duration/markers/terminal snapshot,
/// and a fallback "No file selected" message when `preview_data` is `None`.
pub(super) fn render_preview_panel(
    buf: &mut Buffer,
    area: Rect,
    preview_data: Option<PreviewData>,
    session_preview_data: Option<SessionData>,
    has_backup: bool,
    theme: &crate::theme::Theme,
) {
    let lines = preview_data.map(|data| {
        let input = PreviewInput::new(data, session_preview_data, has_backup);
        build_preview_lines(input, theme)
    });
    let preview_text = lines.unwrap_or_else(|| vec![Line::from("No file selected")]);
    let preview = Paragraph::new(preview_text).block(preview_block(theme));
    preview.render(area, buf);
}

/// Bordered " Preview " block.
fn preview_block(theme: &crate::theme::Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(" Preview ")
        .border_style(theme.text_secondary_style())
}

/// One `Label: value` line, label dimmed.
fn field_line(
    label: &str,
    value: String,
    value_style: Style,
    theme: &crate::theme::Theme,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_string(), theme.text_secondary_style()),
        Span::styled(value, value_style),
    ])
}

/// Everything the preview panel shows about the selected item.
pub(super) struct PreviewInput {
    pub name: String,
    pub agent: String,
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Local>,
    pub path: String,
    pub lock_data: Option<LockInfo>,
    pub session: Option<SessionData>,
    pub has_backup: bool,
}

impl PreviewInput {
    pub(super) fn new(data: PreviewData, session: Option<SessionData>, has_backup: bool) -> Self {
        let (name, agent, size, modified, path, lock_data) = data;
        Self {
            name,
            agent,
            size,
            modified,
            path,
            lock_data,
            session,
            has_backup,
        }
    }
}

/// Build the list of text lines shown inside the preview panel.
///
/// Includes file name, agent, size (plus uncompressed size for compressed sessions),
/// and conditionally duration, markers, backup status, lock status, modified time,
/// and a terminal snapshot.
pub(super) fn build_preview_lines(
    input: PreviewInput,
    theme: &crate::theme::Theme,
) -> Vec<Line<'static>> {
    let mut lines = head_lines(input.name, input.agent, input.size, theme);
    let Some((duration, markers, styled, uncompressed)) = input.session else {
        lines.extend(fallback_lines(input.modified, input.path, theme));
        return lines;
    };
    let style = theme.text_style();
    lines.extend(uncompressed.map(|u| field_line("Uncompressed: ", u, style, theme)));
    let (modified, lock, backup) = (input.modified, input.lock_data, input.has_backup);
    append_session_preview_lines(
        &mut lines, duration, markers, styled, modified, lock, backup, theme,
    );
    lines
}

/// Name, agent and on-disk size lines.
fn head_lines(
    name: String,
    agent: String,
    size: u64,
    theme: &crate::theme::Theme,
) -> Vec<Line<'static>> {
    vec![
        field_line("Name: ", name, theme.text_style(), theme),
        field_line("Agent: ", agent, theme.accent_style(), theme),
        field_line("Size: ", format_size(size), theme.text_style(), theme),
    ]
}

/// Modified and path lines shown when no session preview is loaded.
fn fallback_lines(
    modified: chrono::DateTime<chrono::Local>,
    path: String,
    theme: &crate::theme::Theme,
) -> Vec<Line<'static>> {
    let time = modified.format("%Y-%m-%d %H:%M").to_string();
    vec![
        field_line("Modified: ", time, theme.text_style(), theme),
        field_line("Path: ", path, theme.text_secondary_style(), theme),
    ]
}

/// Append duration, markers, backup/lock status, modified time, and terminal
/// snapshot lines to an existing preview line buffer.
#[allow(clippy::too_many_arguments)]
pub(super) fn append_session_preview_lines(
    lines: &mut Vec<Line<'static>>,
    duration: String,
    markers: usize,
    styled_preview: Vec<crate::terminal::StyledLine>,
    modified: chrono::DateTime<chrono::Local>,
    lock_data: Option<LockInfo>,
    has_backup: bool,
    theme: &crate::theme::Theme,
) {
    lines.push(Line::from(vec![
        Span::styled("Duration: ", theme.text_secondary_style()),
        Span::styled(duration, theme.text_style()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("Markers: ", theme.text_secondary_style()),
        Span::styled(markers.to_string(), theme.text_style()),
    ]));

    if has_backup {
        lines.push(Line::from(vec![
            Span::styled("Backup: ", theme.text_secondary_style()),
            Span::styled(
                "Available",
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    if let Some(ref lock) = lock_data {
        lines.push(Line::from(vec![
            Span::styled("Status: ", theme.text_secondary_style()),
            Span::styled(
                format!("\u{1F4F9} Recording (PID {})", lock.pid),
                theme.text_secondary_style(),
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("Modified: ", theme.text_secondary_style()),
        Span::styled(
            modified.format("%Y-%m-%d %H:%M").to_string(),
            theme.text_style(),
        ),
    ]));

    if !styled_preview.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Preview",
            theme.text_secondary_style(),
        )]));

        for styled_line in styled_preview.iter().take(12) {
            let mut ratatui_line = SessionPreview::styled_line_to_ratatui(styled_line);
            if let Some(first_span) = ratatui_line.spans.first_mut() {
                *first_span = Span::styled(format!(" {}", first_span.content), first_span.style);
            } else {
                ratatui_line.spans.insert(0, Span::raw(" "));
            }
            lines.push(ratatui_line);
        }
    }
}

/// Format a byte size as human-readable string
pub(super) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
