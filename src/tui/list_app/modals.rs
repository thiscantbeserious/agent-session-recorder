//! Modal overlay rendering for the list app (help, context menu, optimize result).

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::render::*;
use super::*;
use crate::theme::current_theme;
use crate::tui::app::modals::{
    help_close_hint, help_section_header, help_shortcut_line, help_title_line,
    render_help_paragraph,
};

impl ListApp {
    /// Render the help modal overlay.
    /// Public for snapshot testing.
    pub fn render_help_modal(frame: &mut Frame, area: Rect) {
        let accent = current_theme().accent;
        let help_text = vec![
            help_title_line("Keyboard Shortcuts", accent),
            Line::from(""),
            help_section_header("Navigation"),
            help_shortcut_line("  ↑/↓ j/k", "    Navigate", accent),
            help_shortcut_line("  PgUp/Dn", "    Page up/down", accent),
            help_shortcut_line("  Home/End", "   First/last", accent),
            Line::from(""),
            help_section_header("Actions"),
            help_shortcut_line("  Enter", "       Context menu", accent),
            help_shortcut_line("  p", "           Play session", accent),
            help_shortcut_line("  c", "           Copy to clipboard", accent),
            help_shortcut_line("  r", "           Rename session", accent),
            help_shortcut_line("  t", "           Optimize (removes silence)", accent),
            help_shortcut_line("  a", "           Analyze session", accent),
            help_shortcut_line("  d", "           Delete session", accent),
            help_shortcut_line("  Paste", "       Import .cast file(s)", accent),
            Line::from(""),
            help_section_header("Filtering"),
            help_shortcut_line("  /", "           Search by filename", accent),
            help_shortcut_line("  f", "           Filter by agent", accent),
            help_shortcut_line("  Esc", "         Clear filters", accent),
            Line::from(""),
            help_shortcut_line("  ?", "           This help", accent),
            help_shortcut_line("  q", "           Quit", accent),
            Line::from(""),
            help_close_hint(),
        ];
        render_help_paragraph(frame, area, "Help", help_text, 60, 31);
    }

    /// Render the context menu modal overlay.
    ///
    /// This function is public to allow snapshot testing.
    pub fn render_context_menu_modal(
        frame: &mut Frame,
        area: Rect,
        selected_idx: usize,
        backup_exists: bool,
    ) {
        let theme = current_theme();

        // Center the modal
        let modal_width = 40.min(area.width.saturating_sub(4));
        let modal_height = (ContextMenuItem::ALL.len() + 4) as u16; // items + title + padding + footer
        let modal_height = modal_height.min(area.height.saturating_sub(4));
        let x = (area.width - modal_width) / 2;
        let y = (area.height - modal_height) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        // Build menu lines
        let mut lines = vec![
            Line::from(Span::styled(
                "Actions",
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ];

        for (idx, item) in ContextMenuItem::ALL.iter().enumerate() {
            let is_selected = idx == selected_idx;
            let is_restore = matches!(item, ContextMenuItem::Restore);
            let is_disabled = is_restore && !backup_exists;

            let label = build_menu_item_label(item, backup_exists);
            let style = menu_item_style(is_selected, is_disabled, &theme);

            // Add selection indicator
            let prefix = if is_selected { "> " } else { "  " };
            lines.push(Line::from(Span::styled(
                format!("{}{}", prefix, label),
                style,
            )));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "↑↓: navigate | Enter: select | Esc: cancel",
            Style::default().fg(theme.text_secondary),
        )));

        let menu = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent))
                    .title(" Menu "),
            )
            .alignment(Alignment::Left);

        frame.render_widget(menu, modal_area);
    }

    /// Render the optimize result modal overlay.
    ///
    /// This function is public to allow snapshot testing.
    pub fn render_optimize_result_modal(
        frame: &mut Frame,
        area: Rect,
        result_state: &OptimizeResultState,
    ) {
        let theme = current_theme();

        // Determine modal size based on success or error
        let is_success = result_state.result.is_ok();
        let modal_width = 55.min(area.width.saturating_sub(4));
        let modal_height = if is_success { 10 } else { 8 };
        let modal_height = modal_height.min(area.height.saturating_sub(4));

        // Center the modal
        let x = (area.width - modal_width) / 2;
        let y = (area.height - modal_height) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        // Clear the area behind the modal
        frame.render_widget(Clear, modal_area);

        // Build content based on success or error
        let (title, border_color, lines) = match &result_state.result {
            Ok(result) => {
                let title = " Optimization Complete ";
                let border_color = theme.success;

                let lines = vec![
                    Line::from(Span::styled(
                        format!("File: {}", result_state.filename),
                        Style::default().fg(theme.text_primary),
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Original: ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            format_duration(result.original_duration),
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("New:      ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            format_duration(result.new_duration),
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Saved:    ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            format!(
                                "{} ({:.0}%)",
                                format_duration(result.time_saved()),
                                result.percent_saved()
                            ),
                            Style::default()
                                .fg(theme.success)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Backup: ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            if result.backup_created {
                                "Created"
                            } else {
                                "Using existing"
                            },
                            Style::default().fg(theme.text_primary),
                        ),
                    ]),
                ];
                (title, border_color, lines)
            }
            Err(error) => {
                let title = " Optimization Failed ";
                let border_color = theme.error;

                let lines = vec![
                    Line::from(Span::styled(
                        format!("File: {}", result_state.filename),
                        Style::default().fg(theme.text_primary),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Error:",
                        Style::default()
                            .fg(theme.error)
                            .add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(
                        error.to_string(),
                        Style::default().fg(theme.error),
                    )),
                ];
                (title, border_color, lines)
            }
        };

        let modal = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .title(title),
            )
            .wrap(Wrap { trim: false });

        frame.render_widget(modal, modal_area);
    }
}
