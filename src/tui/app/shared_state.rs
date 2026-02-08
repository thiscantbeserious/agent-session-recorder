//! Shared state for TUI applications
//!
//! Contains the fields that are common across all TUI explorer apps
//! (search input, agent filter, explorer, status message, preview cache).

use std::time::Instant;

use crate::config::Config;
use crate::storage::StorageManager;
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
    /// Storage manager for rescanning files from disk
    pub storage: Option<StorageManager>,
    /// Tracks when locks/files were last refreshed
    pub last_lock_refresh: Instant,
}

impl SharedState {
    /// Create a new `SharedState` from a list of file items.
    ///
    /// Collects unique agent names from the items and prepends "All".
    pub fn new(items: Vec<FileItem>, config: Option<Config>) -> Self {
        let storage = config.map(StorageManager::new);

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
            storage,
            last_lock_refresh: Instant::now(),
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

    /// Periodic tick handler — refreshes lock states and file list.
    ///
    /// Called on every `Event::Tick` from the shared event loop.
    /// Only performs work every 3 seconds to minimize overhead.
    pub fn maybe_refresh_tick(&mut self) {
        const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);
        if self.last_lock_refresh.elapsed() < REFRESH_INTERVAL {
            return;
        }
        self.last_lock_refresh = Instant::now();

        // Refresh lock state for currently visible items
        self.explorer.refresh_visible_locks();

        // Rescan file system for new/removed sessions
        self.maybe_refresh_file_list();
    }

    /// Rescan the file system and merge new/removed sessions into the explorer.
    ///
    /// Only runs if a `StorageManager` is available (i.e. Config was provided).
    fn maybe_refresh_file_list(&mut self) {
        let storage = match &self.storage {
            Some(s) => s,
            None => return,
        };

        let fresh_sessions = match storage.list_sessions(None) {
            Ok(sessions) => sessions,
            Err(_) => return,
        };

        let fresh_items: Vec<FileItem> = fresh_sessions.into_iter().map(FileItem::from).collect();
        self.explorer.merge_items(fresh_items);
        self.update_available_agents();
    }

    /// Rebuild the available agents list from the current explorer items.
    ///
    /// Called after merging fresh items so new agents appear in the filter.
    fn update_available_agents(&mut self) {
        let agents = self.explorer.unique_agents();

        let mut new_available = vec!["All".to_string()];
        new_available.extend(agents.into_iter().map(String::from));

        // Preserve current filter selection if possible
        let current_filter = self.available_agents.get(self.agent_filter_idx).cloned();
        self.available_agents = new_available;

        if let Some(ref current) = current_filter {
            if let Some(pos) = self.available_agents.iter().position(|a| a == current) {
                self.agent_filter_idx = pos;
            } else {
                self.agent_filter_idx = 0; // Reset to "All" if agent disappeared
            }
        }
    }
}
