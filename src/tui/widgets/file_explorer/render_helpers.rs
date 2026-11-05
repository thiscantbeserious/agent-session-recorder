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

/// Build spans for an item that is currently being renamed.
///
/// Handles both `selected_all` mode (entire text highlighted) and normal edit mode
/// with a block cursor rendered at the byte position `cursor` within `input`.
///
/// `cursor` is clamped to a valid UTF-8 boundary to prevent panics on caller error.
pub(super) fn build_rename_item_spans<'a>(
    input: &'a str,
    cursor: usize,
    selected_all: bool,
    agent: &str,
    size_str: &str,
    theme: &crate::theme::Theme,
) -> Vec<Span<'a>> {
    // Clamp cursor to valid UTF-8 boundary to prevent panic on caller error
    let cursor = cursor.min(input.len());
    let cursor = if input.is_char_boundary(cursor) {
        cursor
    } else {
        input
            .char_indices()
            .map(|(idx, _)| idx)
            .take_while(|&idx| idx < cursor)
            .last()
            .unwrap_or(0)
    };

    let cursor_style = theme.highlight_style().add_modifier(Modifier::SLOW_BLINK);
    let mut spans: Vec<Span<'a>> = vec![];

    if selected_all {
        spans.push(Span::styled(input, cursor_style));
        spans.push(Span::styled(".cast", theme.text_secondary_style()));
    } else {
        let before = &input[..cursor];
        if !before.is_empty() {
            spans.push(Span::styled(before, theme.text_style()));
        }
        if cursor < input.len() {
            let char_len = input[cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(1);
            spans.push(Span::styled(
                &input[cursor..cursor + char_len],
                cursor_style,
            ));
            let after = &input[cursor + char_len..];
            if !after.is_empty() {
                spans.push(Span::styled(after, theme.text_style()));
            }
            spans.push(Span::styled(".cast", theme.text_secondary_style()));
        } else {
            // Cursor at end: style the "." of ".cast" as cursor
            spans.push(Span::styled(".", cursor_style));
            spans.push(Span::styled("cast", theme.text_secondary_style()));
        }
    }

    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("({}, {})", agent, size_str),
        theme.text_secondary_style(),
    ));
    spans
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
    session_preview_data: Option<(String, usize, Vec<crate::terminal::StyledLine>)>,
    has_backup: bool,
    theme: &crate::theme::Theme,
) {
    let preview_text = if let Some((name, agent, size, modified, path, lock_data)) = preview_data {
        build_preview_lines(
            name,
            agent,
            size,
            modified,
            path,
            lock_data,
            session_preview_data,
            has_backup,
            theme,
        )
    } else {
        vec![Line::from("No file selected")]
    };

    let preview = Paragraph::new(preview_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Preview ")
            .border_style(theme.text_secondary_style()),
    );

    preview.render(area, buf);
}

/// Build the list of text lines shown inside the preview panel.
///
/// Includes file name, agent, size, and conditionally duration, markers,
/// backup status, lock status, modified time, and a terminal snapshot.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_preview_lines(
    name: String,
    agent: String,
    size: u64,
    modified: chrono::DateTime<chrono::Local>,
    path: String,
    lock_data: Option<LockInfo>,
    session_preview_data: Option<(String, usize, Vec<crate::terminal::StyledLine>)>,
    has_backup: bool,
    theme: &crate::theme::Theme,
) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Name: ", theme.text_secondary_style()),
            Span::styled(name, theme.text_style()),
        ]),
        Line::from(vec![
            Span::styled("Agent: ", theme.text_secondary_style()),
            Span::styled(agent, theme.accent_style()),
        ]),
        Line::from(vec![
            Span::styled("Size: ", theme.text_secondary_style()),
            Span::styled(format_size(size), theme.text_style()),
        ]),
    ];

    if let Some((duration, markers, styled_preview)) = session_preview_data {
        append_session_preview_lines(
            &mut lines,
            duration,
            markers,
            styled_preview,
            modified,
            lock_data,
            has_backup,
            theme,
        );
    } else {
        lines.push(Line::from(vec![
            Span::styled("Modified: ", theme.text_secondary_style()),
            Span::styled(
                modified.format("%Y-%m-%d %H:%M").to_string(),
                theme.text_style(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Path: ", theme.text_secondary_style()),
            Span::styled(path, theme.text_secondary_style()),
        ]));
    }

    lines
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
