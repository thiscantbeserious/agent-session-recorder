//! Status line and footer text for the cleanup app.

use super::Mode;

/// Compute the status line text for the current mode.
///
/// Returns a status message if one is set; otherwise builds a summary based on
/// selection count and active filters. Returns an empty string for all modes
/// other than `Mode::Normal`.
pub fn status_line_content(
    mode: Mode,
    status: &Option<String>,
    selected_count: usize,
    selected_size: u64,
    explorer_len: usize,
    search_filter: Option<&str>,
    agent_filter: Option<&str>,
) -> String {
    if let Some(msg) = status {
        return msg.clone();
    }

    match mode {
        Mode::Normal => {
            if selected_count > 0 {
                format!(
                    "{} selected ({}) | {} total sessions",
                    selected_count,
                    format_size(selected_size),
                    explorer_len
                )
            } else {
                build_normal_status(explorer_len, search_filter, agent_filter)
            }
        }
        _ => String::new(),
    }
}

/// Build the status line for Normal mode with no selection.
///
/// Shows active filters as context, or a default hint when none are active.
fn build_normal_status(
    explorer_len: usize,
    search_filter: Option<&str>,
    agent_filter: Option<&str>,
) -> String {
    let mut parts = vec![];
    if let Some(search) = search_filter {
        parts.push(format!("search: \"{}\"", search));
    }
    if let Some(agent) = agent_filter {
        parts.push(format!("agent: {}", agent));
    }
    if parts.is_empty() {
        format!("{} sessions | Space to select", explorer_len)
    } else {
        format!(
            "{} sessions ({}) | Space to select",
            explorer_len,
            parts.join(", ")
        )
    }
}

/// Return the footer keybinding text for the given mode.
///
/// In Normal mode, the text changes based on whether items are selected.
pub fn footer_text_for_mode(mode: Mode, selected_count: usize) -> &'static str {
    match mode {
        Mode::Search => "Esc: cancel | Enter: apply | Backspace: delete",
        Mode::GlobSelect => "Esc: cancel | Enter: select matching | Backspace: delete",
        Mode::AgentFilter => "left/right: change | Enter: apply | Esc: cancel",
        Mode::ConfirmDelete => "y: confirm delete | n/Esc: cancel",
        Mode::ConfirmDeleteFinal => "y: confirm | n/Esc: cancel",
        Mode::Help => "Press any key to close",
        Mode::Normal => {
            if selected_count > 0 {
                "Space: toggle | a: toggle all | Enter: delete selected | Esc: clear | ?: help"
            } else {
                "Space: select | a: all | g: glob | /: search | f: filter | ?: help | q: quit"
            }
        }
    }
}

/// Format a byte size as human-readable string.
pub(super) fn format_size(bytes: u64) -> String {
    humansize::format_size(bytes, humansize::BINARY)
}
