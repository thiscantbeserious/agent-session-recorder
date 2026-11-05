//! Explorer sorting, filtering and item bookkeeping.

use std::collections::HashSet;

use ratatui::widgets::ListState;

use crate::files::backup::has_backup;
use crate::files::lock;

use super::{FileExplorer, FileItem, SortDirection, SortField};

impl FileExplorer {
    /// Get the current sort field
    pub fn sort_field(&self) -> SortField {
        self.sort_field
    }

    /// Get the current sort direction
    pub fn sort_direction(&self) -> SortDirection {
        self.sort_direction
    }

    /// Set sort field (resets direction to descending)
    pub fn set_sort(&mut self, field: SortField) {
        if self.sort_field == field {
            // Toggle direction
            self.sort_direction = match self.sort_direction {
                SortDirection::Ascending => SortDirection::Descending,
                SortDirection::Descending => SortDirection::Ascending,
            };
        } else {
            self.sort_field = field;
            self.sort_direction = SortDirection::Descending;
        }
        self.apply_sort();
        self.selected = 0;
        self.sync_list_state();
    }

    /// Re-sort and re-filter after an item rename, restoring selection to the given path.
    pub(crate) fn reindex_after_rename(&mut self, new_path: &str) {
        self.apply_filter();
        self.apply_sort();
        self.restore_selection_by_path(Some(new_path));
        self.sync_list_state();
    }

    /// Apply current sort to visible indices
    pub(super) fn apply_sort(&mut self) {
        let items = &self.items;
        let dir = self.sort_direction;

        self.visible_indices.sort_by(|&a, &b| {
            let item_a = &items[a];
            let item_b = &items[b];

            let cmp = match self.sort_field {
                SortField::Name => item_a.name.cmp(&item_b.name),
                SortField::Size => item_a.size.cmp(&item_b.size),
                SortField::Date => item_a.modified.cmp(&item_b.modified),
            };

            match dir {
                SortDirection::Ascending => cmp,
                SortDirection::Descending => cmp.reverse(),
            }
        });
    }

    // === Filtering ===

    /// Get the current agent filter
    pub fn agent_filter(&self) -> Option<&str> {
        self.agent_filter.as_deref()
    }

    /// Set agent filter (None = show all)
    pub fn set_agent_filter(&mut self, agent: Option<String>) {
        self.agent_filter = agent;
        self.apply_filter();
        self.apply_sort();
        self.selected = 0;
        self.sync_list_state();
    }

    /// Get the current search filter
    pub fn search_filter(&self) -> Option<&str> {
        self.search_filter.as_deref()
    }

    /// Set the search filter (case-insensitive substring match on filename)
    pub fn set_search_filter(&mut self, search: Option<String>) {
        self.search_filter = search;
        self.apply_filter();
        self.apply_sort();
        self.selected = 0;
        self.sync_list_state();
    }

    /// Clear both search and agent filters
    pub fn clear_filters(&mut self) {
        self.search_filter = None;
        self.agent_filter = None;
        self.apply_filter();
        self.apply_sort();
        self.selected = 0;
        self.sync_list_state();
    }

    /// Remove an item by its path
    ///
    /// Returns true if the item was found and removed.
    pub fn remove_item(&mut self, path: &str) -> bool {
        if let Some(idx) = self.items.iter().position(|item| item.path == path) {
            // Remove from multi-selected if present
            self.multi_selected.remove(&idx);

            // Adjust multi_selected indices for items after the removed one
            self.multi_selected = self
                .multi_selected
                .iter()
                .map(|&i| if i > idx { i - 1 } else { i })
                .collect();

            // Remove the item
            self.items.remove(idx);

            // Rebuild visible indices and adjust selection
            self.apply_filter();
            self.apply_sort();

            // Adjust selection if needed
            if self.selected >= self.visible_indices.len() && !self.visible_indices.is_empty() {
                self.selected = self.visible_indices.len() - 1;
            }
            self.sync_list_state();

            true
        } else {
            false
        }
    }

    /// Apply current filter to rebuild visible indices
    pub(super) fn apply_filter(&mut self) {
        self.visible_indices = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                // Agent filter
                let agent_match = self
                    .agent_filter
                    .as_ref()
                    .map(|f| item.agent == *f)
                    .unwrap_or(true);

                // Search filter (case-insensitive substring match on filename)
                let search_match = self
                    .search_filter
                    .as_ref()
                    .map(|s| item.name.to_lowercase().contains(&s.to_lowercase()))
                    .unwrap_or(true);

                agent_match && search_match
            })
            .map(|(idx, _)| idx)
            .collect();
    }

    /// Get unique agent names from all items
    pub fn unique_agents(&self) -> Vec<&str> {
        let mut agents: Vec<&str> = self.items.iter().map(|i| i.agent.as_str()).collect();
        agents.sort();
        agents.dedup();
        agents
    }

    /// Update metadata for an item by reloading from disk.
    /// Returns true if item was found and updated.
    pub fn update_item_metadata(&mut self, path: &str) -> bool {
        if let Some(idx) = self.items.iter().position(|item| item.path == path) {
            // Reload metadata from disk
            if let Ok(metadata) = std::fs::metadata(path) {
                self.items[idx].size = metadata.len();
                // Also update cached has_backup and lock status
                self.items[idx].has_backup = has_backup(std::path::Path::new(path));
                self.items[idx].lock_info = lock::read_lock(std::path::Path::new(path));
                return true;
            }
        }
        false
    }

    /// Update an item's path and name (e.g., after a rename).
    /// Returns true if the old path was found and the item was updated.
    pub fn update_item_path(&mut self, old_path: &str, new_path: &str) -> bool {
        if let Some(idx) = self.items.iter().position(|item| item.path == old_path) {
            let new_p = std::path::Path::new(new_path);
            self.items[idx].path = new_path.to_string();
            self.items[idx].name = new_p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(new_path)
                .to_string();
            // Reload metadata from disk
            if let Ok(metadata) = std::fs::metadata(new_path) {
                self.items[idx].size = metadata.len();
            }
            self.items[idx].has_backup = has_backup(new_p);
            self.items[idx].lock_info = lock::read_lock(new_p);
            true
        } else {
            false
        }
    }

    /// Refresh lock and size metadata for visible items.
    ///
    /// Called periodically to detect lock state changes and keep in-progress
    /// recording size display current for locked items.
    pub fn refresh_visible_item_metadata(&mut self) {
        for &item_idx in &self.visible_indices {
            let path = self.items[item_idx].path.clone();
            let lock_info = lock::read_lock(std::path::Path::new(&path));
            if lock_info.is_some() {
                if let Ok(metadata) = std::fs::metadata(&path) {
                    self.items[item_idx].size = metadata.len();
                }
            }
            self.items[item_idx].lock_info = lock_info;
        }
    }

    /// Merge fresh items into the explorer, adding new ones and removing stale ones.
    ///
    /// Preserves the current selection by path if possible.
    /// Re-applies current filters and sort order after merge.
    pub fn merge_items(&mut self, fresh_items: Vec<FileItem>) {
        let selected_path = self.selected_item().map(|i| i.path.clone());

        // Save multi-selected paths so we can restore after reindex
        let selected_paths: HashSet<String> = self
            .multi_selected
            .iter()
            .filter_map(|&idx| self.items.get(idx).map(|i| i.path.clone()))
            .collect();

        let fresh_paths: HashSet<String> = fresh_items.iter().map(|i| i.path.clone()).collect();
        let existing_paths: HashSet<String> = self.items.iter().map(|i| i.path.clone()).collect();

        // Remove stale items (in current but not in fresh)
        self.items.retain(|item| fresh_paths.contains(&item.path));

        // Add new items (in fresh but not in current)
        for item in fresh_items {
            if !existing_paths.contains(&item.path) {
                self.items.push(item);
            }
        }

        // Rebuild filters + sort
        self.apply_filter();
        self.apply_sort();

        // Restore multi-select by path
        self.multi_selected.clear();
        for (idx, item) in self.items.iter().enumerate() {
            if selected_paths.contains(&item.path) {
                self.multi_selected.insert(idx);
            }
        }

        // Restore selection by path
        self.restore_selection_by_path(selected_path.as_deref());
        self.sync_list_state();
    }

    /// Restore the selected index to the item with the given path.
    pub(super) fn restore_selection_by_path(&mut self, path: Option<&str>) {
        let Some(target) = path else {
            self.selected = 0;
            return;
        };
        let position = self
            .visible_indices
            .iter()
            .position(|&idx| self.items[idx].path == target);
        self.selected = position.unwrap_or(0);
    }

    // === Rendering helpers ===

    /// Get the list state for ratatui
    pub fn list_state(&mut self) -> &mut ListState {
        &mut self.list_state
    }

    /// Get visible items for rendering
    pub fn visible_items(&self) -> impl Iterator<Item = (usize, &FileItem, bool)> {
        self.visible_indices
            .iter()
            .enumerate()
            .map(|(vis_idx, &item_idx)| {
                let item = &self.items[item_idx];
                let is_selected = self.multi_selected.contains(&item_idx);
                (vis_idx, item, is_selected)
            })
    }
}
