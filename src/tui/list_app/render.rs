//! Free rendering helpers for the list app: menu labels, status line, footer, overlays.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{ContextMenuItem, ListApp, Mode, OptimizeResultState};
use crate::theme::current_theme;
use crate::tui::app::modals::{self};
use crate::tui::app::status_footer::{render_input_line, render_status_line};
use crate::tui::import;
use crate::tui::widgets::FileExplorer;

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

/// Return the confirm-dialog prompt prefix for a mode, or None if the mode is not a confirm mode.
pub(super) fn confirm_prompt_for_mode(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::ConfirmDelete => Some("🗑  Delete? "),
        Mode::ConfirmUnlock => Some("🔓 Force unlock? "),
        Mode::ConfirmDeleteFinal => Some("🗑  Are you sure? "),
        Mode::ConfirmUnlockFinal => Some("🔓 Are you sure? "),
        _ => None,
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
    // Guard: all confirm-dialog modes share the same "(y/n) — name" pattern
    if let Some(prompt) = confirm_prompt_for_mode(mode) {
        render_input_line(frame, area, prompt, &format!("(y/n) — {}", selected_name));
        return;
    }

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
        Mode::Help => ListApp::render_help_modal(frame, area),
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
            ListApp::render_context_menu_modal(frame, area, context_menu_idx, backup_exists);
        }
        Mode::OptimizeResult => {
            if let Some(ref result_state) = optimize_result {
                ListApp::render_optimize_result_modal(frame, area, result_state);
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
