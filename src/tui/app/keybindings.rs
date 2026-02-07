//! Shared key dispatch for TUI applications
//!
//! Provides a unified `handle_shared_key()` dispatcher that handles modes
//! common to all explorer apps (Search, AgentFilter, Help, navigation).
//! App-specific modes are handled by each app after receiving `NotConsumed`.

use crossterm::event::KeyEvent;

use super::shared_state::SharedState;

/// Modes shared across all TUI explorer applications.
///
/// Each app wraps these in its own `Mode` enum and adds
/// app-specific variants (e.g., ContextMenu, GlobSelect).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum KeyResult {
    /// Key was handled by shared logic
    Consumed,
    /// Key was not recognized -- app should handle it
    NotConsumed,
}

/// Dispatch a key event through shared mode handlers.
///
/// Call this first in each app's `handle_key()`. If it returns
/// `KeyResult::NotConsumed`, the app runs its own mode-specific logic.
///
/// Currently a stub that always returns `NotConsumed`.
/// Will be populated in Stage 5 with search, agent filter, help,
/// and navigation key handling extracted from list_app and cleanup_app.
#[allow(dead_code)]
pub fn handle_shared_key(
    _mode: &SharedMode,
    _key: KeyEvent,
    _state: &mut SharedState,
) -> KeyResult {
    KeyResult::NotConsumed
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn shared_mode_debug_format() {
        let mode = SharedMode::AgentFilter;
        let debug = format!("{:?}", mode);
        assert!(debug.contains("AgentFilter"));
    }
}
