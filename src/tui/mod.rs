//! TUI (Text User Interface) module for AGR
//!
//! This module provides terminal-based UI components using ratatui/crossterm.
//! It enables dynamic terminal resize handling and rich interactive interfaces.

// Allow unused code during foundation phase - will be used in later phases
#![allow(dead_code)]

pub mod app;
pub mod cleanup_app;
pub mod event;
pub mod list_app;
pub mod theme;
pub mod ui;
pub mod widgets;

// Re-export theme for convenience
pub use theme::{colorize_help, current_theme};

// Re-export apps for commands
pub use cleanup_app::CleanupApp;
pub use list_app::ListApp;
