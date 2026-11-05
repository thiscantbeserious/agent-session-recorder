//! File explorer widget for AGR
//!
//! An interactive file explorer for browsing and selecting session recordings.
//! Features:
//! - Arrow key navigation
//! - Page up/down, Home/End
//! - Multi-select with space
//! - Sort by date/size/name
//! - Filter by agent
//! - Enhanced preview with duration, markers, and terminal snapshot

mod filtering;
mod item;
mod render_helpers;
mod session_preview;
mod state;
mod widget;

#[cfg(test)]
mod tests;

pub use item::FileItem;
pub use session_preview::SessionPreview;
pub use state::{FileExplorer, SortDirection, SortField};
pub use widget::FileExplorerWidget;
