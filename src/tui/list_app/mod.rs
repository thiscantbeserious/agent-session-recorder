//! List command TUI application
//!
//! Interactive file explorer for browsing and managing session recordings.
//! Features: search, agent filter, play, copy, rename, optimize, analyze, restore, delete, import.

use std::time::Duration;

use anyhow::Result;

use super::app::{App, SharedMode, SharedState};
use super::import;
use super::widgets::FileItem;
use crate::asciicast::TransformResult;
use crate::config::Config;

mod input;
mod keys;
mod modals;
mod mouse;
mod render;
mod session_actions;
mod tui_app;

/// UI mode for the list application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal browsing mode
    #[default]
    Normal,
    /// Search mode - typing filters by filename
    Search,
    /// Agent filter mode - selecting agent to filter by
    AgentFilter,
    /// Help mode - showing keyboard shortcuts
    Help,
    /// Confirm delete mode
    ConfirmDelete,
    /// Second confirmation for delete - "Are you sure?"
    ConfirmDeleteFinal,
    /// Context menu mode - showing actions for selected file
    ContextMenu,
    /// Optimize result mode - showing optimization results or error
    OptimizeResult,
    /// Confirm unlock mode - asking user to confirm force-unlock
    ConfirmUnlock,
    /// Second confirmation for unlock - "Are you sure?"
    ConfirmUnlockFinal,
    /// Rename input mode - typing new filename
    RenameInput,
    /// Import mode - pasting .cast files to import
    Import,
}

impl Mode {
    fn to_shared(self) -> Option<SharedMode> {
        match self {
            Mode::Normal => Some(SharedMode::Normal),
            Mode::Search => Some(SharedMode::Search),
            Mode::AgentFilter => Some(SharedMode::AgentFilter),
            Mode::Help => Some(SharedMode::Help),
            Mode::ConfirmDelete => Some(SharedMode::ConfirmDelete),
            Mode::ContextMenu
            | Mode::OptimizeResult
            | Mode::ConfirmUnlock
            | Mode::ConfirmUnlockFinal
            | Mode::ConfirmDeleteFinal
            | Mode::RenameInput
            | Mode::Import => None,
        }
    }

    fn from_shared(mode: SharedMode) -> Self {
        match mode {
            SharedMode::Normal => Mode::Normal,
            SharedMode::Search => Mode::Search,
            SharedMode::AgentFilter => Mode::AgentFilter,
            SharedMode::Help => Mode::Help,
            SharedMode::ConfirmDelete => Mode::ConfirmDelete,
        }
    }
}

/// Context menu item definition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuItem {
    Play,
    Copy,
    Rename,
    Optimize,
    Analyze,
    Restore,
    Delete,
}

impl ContextMenuItem {
    /// All menu items in display order
    pub const ALL: [ContextMenuItem; 7] = [
        ContextMenuItem::Play,
        ContextMenuItem::Copy,
        ContextMenuItem::Rename,
        ContextMenuItem::Optimize,
        ContextMenuItem::Analyze,
        ContextMenuItem::Restore,
        ContextMenuItem::Delete,
    ];

    /// Get the display label for this menu item
    pub fn label(&self) -> &'static str {
        match self {
            ContextMenuItem::Play => "Play",
            ContextMenuItem::Copy => "Copy to clipboard",
            ContextMenuItem::Rename => "Rename",
            ContextMenuItem::Optimize => "Optimize",
            ContextMenuItem::Analyze => "Analyze",
            ContextMenuItem::Restore => "Restore from backup",
            ContextMenuItem::Delete => "Delete",
        }
    }

    /// Get the shortcut key hint for this menu item
    pub fn shortcut(&self) -> &'static str {
        match self {
            ContextMenuItem::Play => "p",
            ContextMenuItem::Copy => "c",
            ContextMenuItem::Rename => "r",
            ContextMenuItem::Optimize => "t",
            ContextMenuItem::Analyze => "a",
            ContextMenuItem::Restore => "",
            ContextMenuItem::Delete => "d",
        }
    }

    /// Whether this menu item has a keyboard shortcut
    pub fn has_shortcut(&self) -> bool {
        !self.shortcut().is_empty()
    }
}

/// Holds the result of an optimize operation for display in modal.
#[derive(Debug, Clone)]
pub struct OptimizeResultState {
    /// The filename that was optimized
    pub filename: String,
    /// The result (Ok with data or Err with message)
    pub result: Result<TransformResult, String>,
}

/// List application state
pub struct ListApp {
    /// Base app for terminal handling
    app: App,
    /// Shared state (explorer, search, agent filter, preview cache, status)
    shared: SharedState,
    /// Current UI mode
    mode: Mode,
    /// Context menu selected index
    context_menu_idx: usize,
    /// Optimize result for modal display
    optimize_result: Option<OptimizeResultState>,
    /// Rename input buffer (filename stem without extension)
    rename_input: String,
    /// Cursor position within rename_input (byte offset)
    rename_cursor: usize,
    /// Whether the entire rename input is "selected" (first keystroke replaces all)
    rename_selected_all: bool,
    /// Import state for drag-and-drop .cast file imports
    import_state: Option<import::ImportState>,
}

impl ListApp {
    /// Create a new list application with the given sessions.
    pub fn new(items: Vec<FileItem>, config: Config) -> Result<Self> {
        let app = App::new(Duration::from_millis(250))?;
        let shared = SharedState::new(items, Some(config));

        Ok(Self {
            app,
            shared,
            mode: Mode::Normal,
            context_menu_idx: 0,
            optimize_result: None,
            rename_input: String::new(),
            rename_cursor: 0,
            rename_selected_all: false,
            import_state: None,
        })
    }

    /// Set initial agent filter (for CLI argument support)
    pub fn set_agent_filter(&mut self, agent: &str) {
        // Find the agent in available_agents and set the index
        if let Some(idx) = self.shared.available_agents.iter().position(|a| a == agent) {
            self.shared.agent_filter_idx = idx;
            self.shared.apply_agent_filter();
        }
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_render;
