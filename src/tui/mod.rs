//! TUI (Text User Interface) module for AGR
//!
//! This module provides terminal-based UI components using ratatui/crossterm.
//! It enables dynamic terminal resize handling and rich interactive interfaces.

// Allow unused code during foundation phase - will be used in later phases
#![allow(dead_code)]

pub mod app;
pub mod cleanup_app;
pub mod event_bus;
pub mod import;
pub mod list_app;
pub mod lru_cache;
pub mod ui;
pub mod widgets;

/// Internals for the integration tests, not a stable API.
#[doc(hidden)]
pub mod testing {
    pub mod list_app {
        pub use crate::tui::list_app::input::{max_stem_len, rename_stem};
        pub use crate::tui::list_app::render::{
            build_menu_item_label, build_rename_status_spans, confirm_prompt_for_mode,
            footer_text_for_mode, format_duration, menu_item_style,
        };
        pub use crate::tui::list_app::session_actions::newest_session_in;
    }

    pub mod cleanup_app {
        pub use crate::tui::cleanup_app::actions::{delete_paths, deletion_status};
        pub use crate::tui::cleanup_app::glob::{
            glob_match, matches_glob_pattern, parse_glob_pattern,
        };
    }
}

// Re-export apps for commands
pub use cleanup_app::CleanupApp;
pub use list_app::ListApp;
