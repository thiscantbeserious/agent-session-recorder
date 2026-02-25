//! Rendering functions for `ListApp`.
//!
//! All free functions (not methods) for drawing modal overlays, the status line,
//! footer text, and helper utilities. These are separated from `mod.rs` to keep
//! file sizes within the 400-line target.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use super::super::app::{
    modals,
    status_footer::{render_input_line, render_status_line},
};
use super::super::widgets::FileExplorer;
use super::{import, ContextMenuItem, Mode, OptimizeResultState};
use crate::theme::current_theme;

/// Render the help modal overlay.
/// Public for snapshot testing.
pub fn render_help_modal(frame: &mut Frame, area: Rect) {
    let theme = current_theme();

    // Center the modal
    let modal_width = 60.min(area.width.saturating_sub(4));
    let modal_height = 31.min(area.height.saturating_sub(2));
    let x = (area.width - modal_width) / 2;
    let y = (area.height - modal_height) / 2;
    let modal_area = Rect::new(x, y, modal_width, modal_height);

    // Clear the area behind the modal
    frame.render_widget(Clear, modal_area);

    let help_text = vec![
        Line::from(Span::styled(
            "Keyboard Shortcuts",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        // Navigation section
        Line::from(Span::styled(
            "Navigation",
            Style::default().fg(theme.text_secondary),
        )),
        Line::from(vec![
            Span::styled("  ↑/↓ j/k", Style::default().fg(theme.accent)),
            Span::raw("    Navigate"),
        ]),
        Line::from(vec![
            Span::styled("  PgUp/Dn", Style::default().fg(theme.accent)),
            Span::raw("    Page up/down"),
        ]),
        Line::from(vec![
            Span::styled("  Home/End", Style::default().fg(theme.accent)),
            Span::raw("   First/last"),
        ]),
        Line::from(""),
        // Actions section
        Line::from(Span::styled(
            "Actions",
            Style::default().fg(theme.text_secondary),
        )),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(theme.accent)),
            Span::raw("       Context menu"),
        ]),
        Line::from(vec![
            Span::styled("  p", Style::default().fg(theme.accent)),
            Span::raw("           Play session"),
        ]),
        Line::from(vec![
            Span::styled("  c", Style::default().fg(theme.accent)),
            Span::raw("           Copy to clipboard"),
        ]),
        Line::from(vec![
            Span::styled("  r", Style::default().fg(theme.accent)),
            Span::raw("           Rename session"),
        ]),
        Line::from(vec![
            Span::styled("  t", Style::default().fg(theme.accent)),
            Span::raw("           Optimize (removes silence)"),
        ]),
        Line::from(vec![
            Span::styled("  a", Style::default().fg(theme.accent)),
            Span::raw("           Analyze session"),
        ]),
        Line::from(vec![
            Span::styled("  d", Style::default().fg(theme.accent)),
            Span::raw("           Delete session"),
        ]),
        Line::from(vec![
            Span::styled("  Paste", Style::default().fg(theme.accent)),
            Span::raw("       Import .cast file(s)"),
        ]),
        Line::from(""),
        // Filter section
        Line::from(Span::styled(
            "Filtering",
            Style::default().fg(theme.text_secondary),
        )),
        Line::from(vec![
            Span::styled("  /", Style::default().fg(theme.accent)),
            Span::raw("           Search by filename"),
        ]),
        Line::from(vec![
            Span::styled("  f", Style::default().fg(theme.accent)),
            Span::raw("           Filter by agent"),
        ]),
        Line::from(vec![
            Span::styled("  Esc", Style::default().fg(theme.accent)),
            Span::raw("         Clear filters"),
        ]),
        Line::from(""),
        // Other section
        Line::from(vec![
            Span::styled("  ?", Style::default().fg(theme.accent)),
            Span::raw("           This help"),
        ]),
        Line::from(vec![
            Span::styled("  q", Style::default().fg(theme.accent)),
            Span::raw("           Quit"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to close",
            Style::default().fg(theme.text_secondary),
        )),
    ];

    let help = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.accent))
                .title(" Help "),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(help, modal_area);
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

/// Build the display label for a context menu item.
///
/// Appends shortcut hint and, for Restore when no backup exists, a "no backup" warning.
pub(super) fn build_menu_item_label(item: &ContextMenuItem, backup_exists: bool) -> String {
    let is_restore = matches!(item, ContextMenuItem::Restore);
    if is_restore && !backup_exists {
        if item.has_shortcut() {
            format!("  {} ({}) - no backup", item.label(), item.shortcut())
        } else {
            format!("  {} - no backup", item.label())
        }
    } else if item.has_shortcut() {
        format!("  {} ({})", item.label(), item.shortcut())
    } else {
        format!("  {}", item.label())
    }
}

/// Return the style for a context menu item based on selection and disabled state.
pub(super) fn menu_item_style(
    is_selected: bool,
    is_disabled: bool,
    theme: &crate::theme::Theme,
) -> Style {
    if is_selected {
        theme.highlight_style()
    } else if is_disabled {
        Style::default().fg(theme.text_secondary)
    } else {
        Style::default().fg(theme.text_primary)
    }
}

/// Render the status line for the current mode.
///
/// Free function (not a method) because `self.app.draw` holds a mutable borrow of `app`
/// and all required fields are passed as explicit parameters.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_status_line_for_mode(
    frame: &mut Frame,
    area: Rect,
    mode: Mode,
    search_input: &str,
    available_agents: &[String],
    agent_filter_idx: usize,
    rename_input: &str,
    rename_cursor: usize,
    rename_selected_all: bool,
    import_state: &Option<import::ImportState>,
    status: &Option<String>,
    explorer: &mut FileExplorer,
    selected_name: &str,
) {
    match mode {
        Mode::Search => {
            let value = format!("{}_", search_input);
            render_input_line(frame, area, "Search: ", &value);
        }
        Mode::AgentFilter => {
            let agent = &available_agents[agent_filter_idx];
            render_input_line(
                frame,
                area,
                "Filter by agent: ",
                &format!("{} (←/→ to change, Enter to apply)", agent),
            );
        }
        Mode::RenameInput => {
            let theme = current_theme();
            let hl = theme.highlight_style();
            let blink = hl.add_modifier(Modifier::SLOW_BLINK);
            let mut spans = vec![Span::styled(
                "Rename: ",
                Style::default().fg(theme.text_secondary),
            )];
            build_rename_status_spans(
                &mut spans,
                rename_input,
                rename_cursor,
                rename_selected_all,
                hl,
                blink,
            );
            frame.render_widget(Paragraph::new(Line::from(spans)), area);
        }
        Mode::ConfirmDelete => {
            render_input_line(
                frame,
                area,
                "🗑  Delete? ",
                &format!("(y/n) — {}", selected_name),
            );
        }
        Mode::ConfirmUnlock => {
            render_input_line(
                frame,
                area,
                "🔓 Force unlock? ",
                &format!("(y/n) — {}", selected_name),
            );
        }
        Mode::ConfirmDeleteFinal => {
            render_input_line(
                frame,
                area,
                "🗑  Are you sure? ",
                &format!("(y/n) — {}", selected_name),
            );
        }
        Mode::ConfirmUnlockFinal => {
            render_input_line(
                frame,
                area,
                "🔓 Are you sure? ",
                &format!("(y/n) — {}", selected_name),
            );
        }
        Mode::ContextMenu | Mode::OptimizeResult => {
            render_status_line(frame, area, selected_name);
        }
        Mode::Import => {
            if let Some(ref state) = import_state {
                let text = match state.phase {
                    import::ImportPhase::AgentInput => {
                        format!("Import: select agent for {} file(s)", state.file_count())
                    }
                    import::ImportPhase::Importing => {
                        format!("Importing {} file(s)...", state.file_count())
                    }
                    import::ImportPhase::Done => {
                        let success_count =
                            state.results.iter().filter(|r| r.outcome.is_ok()).count();
                        let total_count = state.results.len();
                        format!("Imported {}/{} files", success_count, total_count)
                    }
                };
                render_status_line(frame, area, &text);
            }
        }
        _ => {
            let text = if let Some(msg) = status {
                msg.clone()
            } else {
                match mode {
                    Mode::Normal => {
                        let mut parts = vec![];
                        if let Some(search) = explorer.search_filter() {
                            parts.push(format!("search: \"{}\"", search));
                        }
                        if let Some(agent) = explorer.agent_filter() {
                            parts.push(format!("agent: {}", agent));
                        }
                        if parts.is_empty() {
                            format!("{} sessions", explorer.len())
                        } else {
                            format!("{} sessions ({})", explorer.len(), parts.join(", "))
                        }
                    }
                    _ => String::new(),
                }
            };
            render_status_line(frame, area, &text);
        }
    }
}

/// Build the rename cursor spans for the status line.
///
/// Populates `spans` with the styled before/cursor/after segments.
pub(super) fn build_rename_status_spans<'a>(
    spans: &mut Vec<Span<'a>>,
    rename_input: &'a str,
    rename_cursor: usize,
    rename_selected_all: bool,
    hl: Style,
    blink: Style,
) {
    if rename_selected_all {
        spans.push(Span::styled("[", blink));
        spans.push(Span::styled(rename_input, blink));
        spans.push(Span::styled("]", blink));
    } else {
        let before = &rename_input[..rename_cursor];
        let after = &rename_input[rename_cursor..];
        if !before.is_empty() {
            spans.push(Span::styled(before, hl));
        }
        spans.push(Span::styled("|", blink));
        if !after.is_empty() {
            spans.push(Span::styled(after, hl));
        }
    }
}

/// Return the footer hint text for the current mode.
///
/// Free function — does not need `self`; import_state is only needed for `Mode::Import`.
pub(super) fn footer_text_for_mode(mode: Mode, import_state: &Option<import::ImportState>) -> &str {
    match mode {
        Mode::Search => "Esc: cancel | Enter: apply search | Backspace: delete char",
        Mode::AgentFilter => "←/→: change agent | Enter: apply | Esc: cancel",
        Mode::ConfirmDelete => "y: confirm delete | n/Esc: cancel",
        Mode::ConfirmDeleteFinal => "y: confirm | n/Esc: cancel",
        Mode::ConfirmUnlock => "y: force unlock | n/Esc: cancel",
        Mode::ConfirmUnlockFinal => "y: confirm | n/Esc: cancel",
        Mode::Help => "Press any key to close help",
        Mode::ContextMenu => "↑↓: navigate | Enter: select | Esc: cancel",
        Mode::RenameInput => "Enter: confirm | Esc: cancel | Backspace: delete char",
        Mode::OptimizeResult => "Enter/Esc: dismiss",
        Mode::Import => {
            if let Some(ref state) = import_state {
                match state.phase {
                    import::ImportPhase::AgentInput => {
                        "Enter: confirm | Esc: cancel | Tab: complete | Up/Down: select"
                    }
                    import::ImportPhase::Importing => "Importing...",
                    import::ImportPhase::Done => "Enter/Esc: dismiss",
                }
            } else {
                ""
            }
        }
        Mode::Normal => {
            "↑↓: navigate | Enter: menu | p: play | c: copy | r: rename | t: optimize | a: analyze | d: delete | ?: help | q: quit"
        }
    }
}

/// Render all modal overlays for the current mode.
///
/// Free function — does not need `self`; all required state is passed as parameters.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_modal_overlays(
    frame: &mut Frame,
    area: Rect,
    mode: Mode,
    explorer: &mut FileExplorer,
    context_menu_idx: usize,
    backup_exists: bool,
    optimize_result: &Option<OptimizeResultState>,
    import_state: &Option<import::ImportState>,
) {
    match mode {
        Mode::Help => render_help_modal(frame, area),
        Mode::ConfirmDelete => {
            if let Some(item) = explorer.selected_item() {
                modals::render_confirm_delete_modal(frame, area, 1, item.size, false);
            }
        }
        Mode::ConfirmDeleteFinal => {
            if let Some(item) = explorer.selected_item() {
                modals::render_confirm_delete_modal(frame, area, 1, item.size, true);
            }
        }
        Mode::ContextMenu => {
            render_context_menu_modal(frame, area, context_menu_idx, backup_exists);
        }
        Mode::OptimizeResult => {
            if let Some(ref result_state) = optimize_result {
                render_optimize_result_modal(frame, area, result_state);
            }
        }
        Mode::ConfirmUnlock | Mode::ConfirmUnlockFinal => {
            if let Some(item) = explorer.selected_item() {
                let lock_msg = if let Some(ref info) = item.lock_info {
                    let started = info.started.get(..19).unwrap_or(info.started.as_str());
                    format!("PID {} since {}", info.pid, started)
                } else {
                    "Unknown lock".to_string()
                };
                let final_confirm = mode == Mode::ConfirmUnlockFinal;
                modals::render_confirm_unlock_modal(frame, area, &lock_msg, final_confirm);
            }
        }
        Mode::Import => {
            if let Some(ref state) = import_state {
                import::render(state, frame, area);
            }
        }
        _ => {}
    }
}

/// Format a duration in seconds as human-readable string.
///
/// Examples:
/// - 65.5 -> "1m 5s"
/// - 3661.0 -> "1h 1m 1s"
/// - 30.0 -> "30s"
pub(super) fn format_duration(seconds: f64) -> String {
    let total_secs = seconds.max(0.0).round() as u64;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(30.0), "30s");
        assert_eq!(format_duration(0.0), "0s");
        assert_eq!(format_duration(59.4), "59s"); // rounds down
    }

    #[test]
    fn format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(60.0), "1m 0s");
        assert_eq!(format_duration(90.0), "1m 30s");
        assert_eq!(format_duration(3599.0), "59m 59s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3600.0), "1h 0m 0s");
        assert_eq!(format_duration(3661.0), "1h 1m 1s");
        assert_eq!(format_duration(7322.0), "2h 2m 2s");
    }

    #[test]
    fn format_duration_negative_input_clamps_to_zero() {
        assert_eq!(format_duration(-5.0), "0s");
    }

    #[test]
    fn format_duration_nan_clamps_to_zero() {
        assert_eq!(format_duration(f64::NAN), "0s");
    }

    // ── footer_text_for_mode ──────────────────────────────────────────────

    #[test]
    fn footer_text_normal_mode() {
        let text = footer_text_for_mode(Mode::Normal, &None);
        assert!(text.contains("navigate"));
        assert!(text.contains("play"));
        assert!(text.contains("quit"));
    }

    #[test]
    fn footer_text_search_mode() {
        let text = footer_text_for_mode(Mode::Search, &None);
        assert!(text.contains("Esc"));
        assert!(text.contains("search"));
    }

    #[test]
    fn footer_text_confirm_delete_mode() {
        let text = footer_text_for_mode(Mode::ConfirmDelete, &None);
        assert!(text.contains("confirm delete"));
    }

    #[test]
    fn footer_text_confirm_delete_final_mode() {
        let text = footer_text_for_mode(Mode::ConfirmDeleteFinal, &None);
        assert!(text.contains("confirm"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn footer_text_confirm_unlock_mode() {
        let text = footer_text_for_mode(Mode::ConfirmUnlock, &None);
        assert!(text.contains("force unlock"));
    }

    #[test]
    fn footer_text_rename_input_mode() {
        let text = footer_text_for_mode(Mode::RenameInput, &None);
        assert!(text.contains("confirm"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn footer_text_optimize_result_mode() {
        let text = footer_text_for_mode(Mode::OptimizeResult, &None);
        assert!(text.contains("dismiss"));
    }

    #[test]
    fn footer_text_import_none_state() {
        // Import mode with no state yields empty string
        let text = footer_text_for_mode(Mode::Import, &None);
        assert_eq!(text, "");
    }

    #[test]
    fn footer_text_import_agent_input_phase() {
        let agents: Vec<String> = vec![];
        let mut state = import::ImportState::new("/tmp/test.cast", &agents);
        state.phase = import::ImportPhase::AgentInput;
        let opt = Some(state);
        let text = footer_text_for_mode(Mode::Import, &opt);
        assert!(text.contains("Enter"));
        assert!(text.contains("Tab"));
    }

    #[test]
    fn footer_text_import_importing_phase() {
        let agents: Vec<String> = vec![];
        let mut state = import::ImportState::new("/tmp/test.cast", &agents);
        state.phase = import::ImportPhase::Importing;
        let opt = Some(state);
        let text = footer_text_for_mode(Mode::Import, &opt);
        assert!(text.contains("Importing"));
    }

    #[test]
    fn footer_text_import_done_phase() {
        let agents: Vec<String> = vec![];
        let mut state = import::ImportState::new("/tmp/test.cast", &agents);
        state.phase = import::ImportPhase::Done;
        let opt = Some(state);
        let text = footer_text_for_mode(Mode::Import, &opt);
        assert!(text.contains("dismiss"));
    }

    // ── build_menu_item_label ─────────────────────────────────────────────

    #[test]
    fn menu_item_label_normal_item_with_shortcut() {
        // Play has shortcut "p"
        let label = build_menu_item_label(&ContextMenuItem::Play, true);
        assert!(label.contains("Play"));
        assert!(label.contains("(p)"));
        assert!(!label.contains("no backup"));
    }

    #[test]
    fn menu_item_label_restore_with_backup() {
        let label = build_menu_item_label(&ContextMenuItem::Restore, true);
        assert!(label.contains("Restore"));
        assert!(!label.contains("no backup"));
    }

    #[test]
    fn menu_item_label_restore_no_backup() {
        let label = build_menu_item_label(&ContextMenuItem::Restore, false);
        assert!(label.contains("Restore"));
        assert!(label.contains("no backup"));
    }

    #[test]
    fn menu_item_label_copy_item() {
        let label = build_menu_item_label(&ContextMenuItem::Copy, true);
        assert!(label.contains("Copy to clipboard"));
        assert!(label.contains("(c)"));
    }

    // ── menu_item_style ───────────────────────────────────────────────────

    #[test]
    fn menu_item_style_selected_uses_highlight() {
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(true, false, &theme);
        let expected = theme.highlight_style();
        assert_eq!(style, expected);
    }

    #[test]
    fn menu_item_style_disabled_uses_secondary_color() {
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(false, true, &theme);
        assert_eq!(style.fg, Some(theme.text_secondary));
    }

    #[test]
    fn menu_item_style_normal_uses_primary_color() {
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(false, false, &theme);
        assert_eq!(style.fg, Some(theme.text_primary));
    }

    #[test]
    fn menu_item_style_selected_takes_precedence_over_disabled() {
        // selected=true, disabled=true → still highlight (selected wins)
        let theme = crate::theme::Theme::claude_code();
        let style = menu_item_style(true, true, &theme);
        let expected = theme.highlight_style();
        assert_eq!(style, expected);
    }

    // ── build_rename_status_spans ─────────────────────────────────────────

    #[test]
    fn rename_spans_selected_all_wraps_in_brackets() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        build_rename_status_spans(&mut spans, "hello", 0, true, hl, blink);
        // Expect 3 spans: "[", "hello", "]" — all with blink style
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "[");
        assert_eq!(spans[1].content, "hello");
        assert_eq!(spans[2].content, "]");
        assert_eq!(spans[0].style, blink);
    }

    #[test]
    fn rename_spans_cursor_at_start_no_before_span() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // cursor=0, selected_all=false → only "|" + "hello"
        build_rename_status_spans(&mut spans, "hello", 0, false, hl, blink);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "|");
        assert_eq!(spans[1].content, "hello");
    }

    #[test]
    fn rename_spans_cursor_at_end_no_after_span() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // cursor at end → "hello" + "|"
        build_rename_status_spans(&mut spans, "hello", 5, false, hl, blink);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "hello");
        assert_eq!(spans[1].content, "|");
    }

    #[test]
    fn rename_spans_cursor_in_middle_three_spans() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // cursor=2 in "hello" → "he" + "|" + "llo"
        build_rename_status_spans(&mut spans, "hello", 2, false, hl, blink);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "he");
        assert_eq!(spans[1].content, "|");
        assert_eq!(spans[2].content, "llo");
    }

    #[test]
    fn rename_spans_empty_input_cursor_only() {
        let hl = ratatui::style::Style::default();
        let blink =
            ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // empty input → only "|"
        build_rename_status_spans(&mut spans, "", 0, false, hl, blink);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "|");
    }

    #[test]
    fn rename_spans_multibyte_cursor_at_char_boundary() {
        let theme = crate::theme::Theme::claude_code();
        let hl = theme.highlight_style();
        let blink = Style::default().add_modifier(Modifier::SLOW_BLINK);
        let mut spans = vec![];
        // é is 2 bytes in UTF-8
        let input = "\u{00e9}llo"; // 4 bytes total
        build_rename_status_spans(&mut spans, input, 2, false, hl, blink);
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "\u{00e9}");
        assert_eq!(spans[1].content, "|");
        assert_eq!(spans[2].content, "llo");
    }
}
