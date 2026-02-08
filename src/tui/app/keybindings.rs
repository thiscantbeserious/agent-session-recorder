//! Shared key dispatch for TUI applications
//!
//! Provides a unified `handle_shared_key()` dispatcher that handles modes
//! common to all explorer apps (Search, AgentFilter, Help, navigation).
//! App-specific modes are handled by each app after receiving `NotConsumed`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::shared_state::SharedState;

/// Modes shared across all TUI explorer applications.
///
/// Each app wraps these in its own `Mode` enum and adds
/// app-specific variants (e.g., ContextMenu, GlobSelect).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharedMode {
    /// Normal browsing mode
    Normal,
    /// Search mode -- typing filters by filename
    Search,
    /// Agent filter mode -- cycling through agents
    AgentFilter,
    /// Help mode -- showing keyboard shortcuts
    Help,
    /// Confirm delete mode
    ConfirmDelete,
}

/// Result of shared key handling.
///
/// `Consumed` means the key was fully handled by shared logic.
/// `NotConsumed` means the app should process the key itself.
/// `EnterMode` signals that a mode transition should occur (caller decides).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyResult {
    /// Key was handled by shared logic
    Consumed,
    /// Key was not recognized -- app should handle it
    NotConsumed,
    /// Shared logic requests a mode transition (caller sets the mode)
    EnterMode(SharedMode),
}

/// Dispatch a key event through shared mode handlers.
///
/// Call this first in each app's `handle_key()`. If it returns
/// `KeyResult::NotConsumed`, the app runs its own mode-specific logic.
/// If it returns `KeyResult::EnterMode`, the app should transition
/// to the requested mode.
pub fn handle_shared_key(mode: &SharedMode, key: KeyEvent, state: &mut SharedState) -> KeyResult {
    match mode {
        SharedMode::Search => handle_search_key(key, state),
        SharedMode::AgentFilter => handle_agent_filter_key(key, state),
        SharedMode::Help => handle_help_key(),
        SharedMode::Normal => handle_normal_navigation(key, state),
        SharedMode::ConfirmDelete => KeyResult::NotConsumed,
    }
}

/// Handle keys in search mode.
///
/// Enter commits the search filter, Escape cancels, Backspace deletes
/// a character, and printable characters append to the search input.
/// All keystrokes perform live filtering as the user types.
fn handle_search_key(key: KeyEvent, state: &mut SharedState) -> KeyResult {
    match key.code {
        KeyCode::Esc => KeyResult::EnterMode(SharedMode::Normal),
        KeyCode::Enter => {
            apply_search_filter(state);
            KeyResult::EnterMode(SharedMode::Normal)
        }
        KeyCode::Backspace => {
            state.search_input.pop();
            apply_search_filter(state);
            KeyResult::Consumed
        }
        KeyCode::Char(c) => {
            // Ignore ctrl+c etc in search mode
            if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                state.search_input.push(c);
                apply_search_filter(state);
            }
            KeyResult::Consumed
        }
        _ => KeyResult::Consumed,
    }
}

/// Apply the search filter from the current search input.
fn apply_search_filter(state: &mut SharedState) {
    if state.search_input.is_empty() {
        state.explorer.set_search_filter(None);
    } else {
        state
            .explorer
            .set_search_filter(Some(state.search_input.clone()));
    }
}

/// Handle keys in agent filter mode.
///
/// Left/h and Right/l cycle through available agents.
/// Enter and Escape exit back to normal mode.
fn handle_agent_filter_key(key: KeyEvent, state: &mut SharedState) -> KeyResult {
    match key.code {
        KeyCode::Esc | KeyCode::Enter => KeyResult::EnterMode(SharedMode::Normal),
        KeyCode::Left | KeyCode::Char('h') => {
            if state.agent_filter_idx > 0 {
                state.agent_filter_idx -= 1;
            } else {
                state.agent_filter_idx = state.available_agents.len() - 1;
            }
            state.apply_agent_filter();
            KeyResult::Consumed
        }
        KeyCode::Right | KeyCode::Char('l') => {
            state.agent_filter_idx = (state.agent_filter_idx + 1) % state.available_agents.len();
            state.apply_agent_filter();
            KeyResult::Consumed
        }
        _ => KeyResult::Consumed,
    }
}

/// Handle keys in help mode.
///
/// Any key exits help and returns to normal mode.
fn handle_help_key() -> KeyResult {
    KeyResult::EnterMode(SharedMode::Normal)
}

/// Handle navigation and mode-transition keys in normal mode.
///
/// Handles only keys that are identical between list_app and cleanup_app:
/// navigation (up/down/pgup/pgdn/home/end) and mode transitions
/// ('/' for search, 'f' for agent filter, '?' for help).
/// Returns `NotConsumed` for app-specific keys (Enter, Space, etc.).
fn handle_normal_navigation(key: KeyEvent, state: &mut SharedState) -> KeyResult {
    match key.code {
        // Navigation
        KeyCode::Up | KeyCode::Char('k') => {
            state.explorer.up();
            KeyResult::Consumed
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.explorer.down();
            KeyResult::Consumed
        }
        KeyCode::PageUp => {
            state.explorer.page_up();
            KeyResult::Consumed
        }
        KeyCode::PageDown => {
            state.explorer.page_down();
            KeyResult::Consumed
        }
        KeyCode::Home => {
            state.explorer.home();
            KeyResult::Consumed
        }
        KeyCode::End => {
            state.explorer.end();
            KeyResult::Consumed
        }

        // Mode transitions (shared across both apps)
        KeyCode::Char('/') => {
            state.search_input.clear();
            state.status_message = None;
            KeyResult::EnterMode(SharedMode::Search)
        }
        KeyCode::Char('f') => {
            if state.available_agents.len() <= 1 {
                state.status_message = Some("No agents to filter by".to_string());
                return KeyResult::Consumed;
            }
            // Set agent_filter_idx based on current filter
            if let Some(current) = state.explorer.agent_filter() {
                state.agent_filter_idx = state
                    .available_agents
                    .iter()
                    .position(|a| a == current)
                    .unwrap_or(0);
            } else {
                state.agent_filter_idx = 0; // "All"
            }
            KeyResult::EnterMode(SharedMode::AgentFilter)
        }
        KeyCode::Char('?') => KeyResult::EnterMode(SharedMode::Help),

        // All other keys are app-specific
        _ => KeyResult::NotConsumed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::widgets::FileItem;
    use chrono::Local;

    fn make_state(agents: &[&str]) -> SharedState {
        let items: Vec<FileItem> = agents
            .iter()
            .enumerate()
            .map(|(i, agent)| FileItem {
                path: format!("/tmp/session_{}.cast", i),
                name: format!("session_{}.cast", i),
                agent: agent.to_string(),
                size: 1024,
                modified: Local::now(),
                has_backup: false,
                lock_info: None,
            })
            .collect();
        SharedState::new(items, None)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn key_with_shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    // --- SharedMode tests ---

    #[test]
    fn shared_mode_equality() {
        assert_eq!(SharedMode::Normal, SharedMode::Normal);
        assert_ne!(SharedMode::Normal, SharedMode::Search);
    }

    #[test]
    fn key_result_variants() {
        assert_eq!(KeyResult::Consumed, KeyResult::Consumed);
        assert_ne!(KeyResult::Consumed, KeyResult::NotConsumed);
    }

    #[test]
    fn key_result_enter_mode_variant() {
        let result = KeyResult::EnterMode(SharedMode::Search);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Search));
        assert_ne!(result, KeyResult::Consumed);
        assert_ne!(result, KeyResult::NotConsumed);
    }

    #[test]
    fn shared_mode_debug_format() {
        let mode = SharedMode::AgentFilter;
        let debug = format!("{:?}", mode);
        assert!(debug.contains("AgentFilter"));
    }

    // --- Search mode tests ---

    #[test]
    fn search_esc_returns_enter_normal() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Search, key(KeyCode::Esc), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Normal));
    }

    #[test]
    fn search_enter_applies_filter_and_returns_normal() {
        let mut state = make_state(&["claude"]);
        state.search_input = "test".to_string();
        let result = handle_shared_key(&SharedMode::Search, key(KeyCode::Enter), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Normal));
        assert_eq!(state.explorer.search_filter(), Some("test"));
    }

    #[test]
    fn search_enter_clears_filter_when_empty() {
        let mut state = make_state(&["claude"]);
        state.search_input.clear();
        let result = handle_shared_key(&SharedMode::Search, key(KeyCode::Enter), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Normal));
        assert_eq!(state.explorer.search_filter(), None);
    }

    #[test]
    fn search_char_appends_and_live_filters() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Search, key(KeyCode::Char('a')), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.search_input, "a");
        assert_eq!(state.explorer.search_filter(), Some("a"));
    }

    #[test]
    fn search_shift_char_appends() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(
            &SharedMode::Search,
            key_with_shift(KeyCode::Char('A')),
            &mut state,
        );
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.search_input, "A");
    }

    #[test]
    fn search_backspace_pops_and_live_filters() {
        let mut state = make_state(&["claude"]);
        state.search_input = "ab".to_string();
        let result = handle_shared_key(&SharedMode::Search, key(KeyCode::Backspace), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.search_input, "a");
        assert_eq!(state.explorer.search_filter(), Some("a"));
    }

    #[test]
    fn search_backspace_on_empty_clears_filter() {
        let mut state = make_state(&["claude"]);
        state.search_input.clear();
        let result = handle_shared_key(&SharedMode::Search, key(KeyCode::Backspace), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.explorer.search_filter(), None);
    }

    // --- Agent filter mode tests ---

    #[test]
    fn agent_filter_esc_returns_enter_normal() {
        let mut state = make_state(&["claude", "copilot"]);
        let result = handle_shared_key(&SharedMode::AgentFilter, key(KeyCode::Esc), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Normal));
    }

    #[test]
    fn agent_filter_enter_returns_enter_normal() {
        let mut state = make_state(&["claude", "copilot"]);
        let result = handle_shared_key(&SharedMode::AgentFilter, key(KeyCode::Enter), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Normal));
    }

    #[test]
    fn agent_filter_right_cycles_forward() {
        let mut state = make_state(&["claude", "copilot"]);
        // available_agents: ["All", "claude", "copilot"]
        assert_eq!(state.agent_filter_idx, 0);
        let result = handle_shared_key(&SharedMode::AgentFilter, key(KeyCode::Right), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.agent_filter_idx, 1);
    }

    #[test]
    fn agent_filter_right_wraps() {
        let mut state = make_state(&["claude"]);
        // available_agents: ["All", "claude"]
        state.agent_filter_idx = 1;
        let result = handle_shared_key(&SharedMode::AgentFilter, key(KeyCode::Right), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.agent_filter_idx, 0);
    }

    #[test]
    fn agent_filter_left_cycles_backward() {
        let mut state = make_state(&["claude", "copilot"]);
        state.agent_filter_idx = 2;
        let result = handle_shared_key(&SharedMode::AgentFilter, key(KeyCode::Left), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.agent_filter_idx, 1);
    }

    #[test]
    fn agent_filter_left_wraps() {
        let mut state = make_state(&["claude", "copilot"]);
        state.agent_filter_idx = 0;
        let result = handle_shared_key(&SharedMode::AgentFilter, key(KeyCode::Left), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.agent_filter_idx, 2); // wraps to last
    }

    // --- Help mode tests ---

    #[test]
    fn help_any_key_returns_enter_normal() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Help, key(KeyCode::Char('x')), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Normal));
    }

    #[test]
    fn help_esc_returns_enter_normal() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Help, key(KeyCode::Esc), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Normal));
    }

    // --- Normal mode navigation tests ---

    #[test]
    fn normal_up_consumed() {
        let mut state = make_state(&["claude", "claude"]);
        state.explorer.down(); // move to item 1
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Up), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.explorer.selected(), 0);
    }

    #[test]
    fn normal_down_consumed() {
        let mut state = make_state(&["claude", "claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Down), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.explorer.selected(), 1);
    }

    #[test]
    fn normal_home_consumed() {
        let mut state = make_state(&["claude", "claude"]);
        state.explorer.end();
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Home), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.explorer.selected(), 0);
    }

    #[test]
    fn normal_end_consumed() {
        let mut state = make_state(&["claude", "claude", "claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::End), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.explorer.selected(), 2);
    }

    #[test]
    fn normal_page_up_consumed() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::PageUp), &mut state);
        assert_eq!(result, KeyResult::Consumed);
    }

    #[test]
    fn normal_page_down_consumed() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::PageDown), &mut state);
        assert_eq!(result, KeyResult::Consumed);
    }

    // --- Normal mode transition tests ---

    #[test]
    fn normal_slash_enters_search() {
        let mut state = make_state(&["claude"]);
        state.search_input = "old".to_string();
        state.status_message = Some("old msg".to_string());
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Char('/')), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Search));
        assert!(state.search_input.is_empty());
        assert!(state.status_message.is_none());
    }

    #[test]
    fn normal_f_enters_agent_filter() {
        let mut state = make_state(&["claude", "copilot"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Char('f')), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::AgentFilter));
        assert_eq!(state.agent_filter_idx, 0); // defaults to "All"
    }

    #[test]
    fn normal_f_preserves_current_agent_filter_idx() {
        let mut state = make_state(&["claude", "copilot"]);
        // Set the filter to "claude" first
        state.agent_filter_idx = 1;
        state.apply_agent_filter();
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Char('f')), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::AgentFilter));
        assert_eq!(state.agent_filter_idx, 1); // preserves "claude"
    }

    #[test]
    fn normal_question_enters_help() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Char('?')), &mut state);
        assert_eq!(result, KeyResult::EnterMode(SharedMode::Help));
    }

    #[test]
    fn normal_enter_not_consumed() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Enter), &mut state);
        assert_eq!(result, KeyResult::NotConsumed);
    }

    #[test]
    fn normal_space_not_consumed() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Char(' ')), &mut state);
        assert_eq!(result, KeyResult::NotConsumed);
    }

    #[test]
    fn normal_esc_not_consumed() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Esc), &mut state);
        assert_eq!(result, KeyResult::NotConsumed);
    }

    // --- ConfirmDelete passthrough ---

    #[test]
    fn confirm_delete_not_consumed() {
        let mut state = make_state(&["claude"]);
        let result = handle_shared_key(
            &SharedMode::ConfirmDelete,
            key(KeyCode::Char('y')),
            &mut state,
        );
        assert_eq!(result, KeyResult::NotConsumed);
    }

    // --- Vim-style key bindings ---

    #[test]
    fn normal_k_navigates_up() {
        let mut state = make_state(&["claude", "claude"]);
        state.explorer.down();
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Char('k')), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.explorer.selected(), 0);
    }

    #[test]
    fn normal_j_navigates_down() {
        let mut state = make_state(&["claude", "claude"]);
        let result = handle_shared_key(&SharedMode::Normal, key(KeyCode::Char('j')), &mut state);
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.explorer.selected(), 1);
    }

    #[test]
    fn agent_filter_h_cycles_left() {
        let mut state = make_state(&["claude", "copilot"]);
        state.agent_filter_idx = 1;
        let result = handle_shared_key(
            &SharedMode::AgentFilter,
            key(KeyCode::Char('h')),
            &mut state,
        );
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.agent_filter_idx, 0);
    }

    #[test]
    fn agent_filter_l_cycles_right() {
        let mut state = make_state(&["claude", "copilot"]);
        let result = handle_shared_key(
            &SharedMode::AgentFilter,
            key(KeyCode::Char('l')),
            &mut state,
        );
        assert_eq!(result, KeyResult::Consumed);
        assert_eq!(state.agent_filter_idx, 1);
    }
}
