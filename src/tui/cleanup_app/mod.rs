//! Cleanup command TUI application
//!
//! Interactive file explorer for selecting and deleting session recordings.
//! Features: multi-select, search, agent filter, glob select, storage preview.

use std::time::Duration;

use anyhow::Result;

use super::app::{App, SharedMode, SharedState};
use super::widgets::FileItem;
use crate::config::Config;

mod actions;
mod glob;
mod keys;
mod render;
mod status;
mod tui_app;

pub use status::{footer_text_for_mode, status_line_content};

/// UI mode for the cleanup application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Normal browsing mode
    #[default]
    Normal,
    /// Search mode - typing filters by filename
    Search,
    /// Agent filter mode - selecting agent to filter by
    AgentFilter,
    /// Glob select mode - enter pattern to select matching files
    GlobSelect,
    /// Help mode - showing keyboard shortcuts
    Help,
    /// Confirm delete mode
    ConfirmDelete,
    /// Second confirmation for delete - "Are you sure?"
    ConfirmDeleteFinal,
}

impl Mode {
    fn to_shared(self) -> Option<SharedMode> {
        match self {
            Mode::Normal => Some(SharedMode::Normal),
            Mode::Search => Some(SharedMode::Search),
            Mode::AgentFilter => Some(SharedMode::AgentFilter),
            Mode::Help => Some(SharedMode::Help),
            Mode::ConfirmDelete => Some(SharedMode::ConfirmDelete),
            Mode::GlobSelect | Mode::ConfirmDeleteFinal => None,
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

/// Cleanup application state
pub struct CleanupApp {
    /// Base app for terminal handling
    app: App,
    /// Shared state (explorer, search, agent filter, preview cache, status)
    shared: SharedState,
    /// Current UI mode
    mode: Mode,
    /// Glob pattern input buffer
    glob_input: String,
    /// Whether files were deleted (for success message)
    files_deleted: bool,
}

impl CleanupApp {
    /// Create a new cleanup application with the given sessions.
    pub fn new(items: Vec<FileItem>, config: Config) -> Result<Self> {
        let app = App::new(Duration::from_millis(250))?;
        let shared = SharedState::new(items, Some(config));

        Ok(Self {
            app,
            shared,
            mode: Mode::Normal,
            glob_input: String::new(),
            files_deleted: false,
        })
    }

    /// Check if any files were deleted during this session
    pub fn files_were_deleted(&self) -> bool {
        self.files_deleted
    }
}

#[cfg(test)]
mod tests;
