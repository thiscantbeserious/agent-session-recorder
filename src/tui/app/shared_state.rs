//! Shared state for TUI applications
//!
//! Contains the fields that are common across all TUI explorer apps
//! (search input, agent filter, explorer, status message, preview cache).

use crate::tui::lru_cache::{new_preview_cache, PreviewCache};
use crate::tui::widgets::{FileExplorer, FileItem, SessionPreview};

/// Shared state fields used by all TUI explorer applications.
///
/// Each app owns a `SharedState` and passes `&mut SharedState` to
/// shared handler functions (keybindings, layout, rendering).
pub struct SharedState {
    /// File explorer widget (navigation, selection, sorting, filtering)
    pub explorer: FileExplorer,
    /// Current search input buffer
    pub search_input: String,
    /// Index into `available_agents` for agent filter cycling
    pub agent_filter_idx: usize,
    /// Agent names available for filtering (first entry is "All")
    pub available_agents: Vec<String>,
    /// Transient status message displayed in the status bar
    pub status_message: Option<String>,
    /// Async LRU cache for session preview loading
    pub preview_cache: PreviewCache,
}

impl SharedState {
    /// Create a new `SharedState` from a list of file items.
    ///
    /// Collects unique agent names from the items and prepends "All".
    pub fn new(items: Vec<FileItem>) -> Self {
        let mut available_agents: Vec<String> = vec!["All".to_string()];
        let mut agents: Vec<String> = items.iter().map(|i| i.agent.clone()).collect();
        agents.sort();
        agents.dedup();
        available_agents.extend(agents);

        let explorer = FileExplorer::new(items);
        let mut preview_cache = new_preview_cache();

        // Synchronously load the first preview so it's available on
        // the very first draw() — no async round-trip needed.
        if let Some(item) = explorer.selected_item() {
            if let Some(preview) = SessionPreview::load(&item.path) {
                preview_cache.insert(item.path.clone(), preview);
            }
        }

        Self {
            explorer,
            search_input: String::new(),
            agent_filter_idx: 0,
            available_agents,
            status_message: None,
            preview_cache,
        }
    }

    /// Apply the currently selected agent filter to the explorer.
    ///
    /// If the selected agent is "All", clears the filter.
    pub fn apply_agent_filter(&mut self) {
        let selected = &self.available_agents[self.agent_filter_idx];
        if selected == "All" {
            self.explorer.set_agent_filter(None);
        } else {
            self.explorer.set_agent_filter(Some(selected.clone()));
        }
    }
}
