//! Explorer state: sorting enums, struct, navigation and selection.

use std::collections::HashSet;

use ratatui::widgets::ListState;

use super::FileItem;

/// Sort field for file list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortField {
    /// Sort by filename
    Name,
    /// Sort by file size
    Size,
    /// Sort by modification date (default)
    #[default]
    Date,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    /// Ascending order
    Ascending,
    /// Descending order (default - newest/largest first)
    #[default]
    Descending,
}

/// File explorer widget state
#[derive(Debug, Clone)]
pub struct FileExplorer {
    /// All items (unfiltered)
    pub(super) items: Vec<FileItem>,
    /// Filtered and sorted items (indices into `items`)
    pub(super) visible_indices: Vec<usize>,
    /// Currently selected index (in visible_indices)
    pub(super) selected: usize,
    /// Set of selected indices for multi-select (indices into `items`)
    pub(super) multi_selected: HashSet<usize>,
    /// Current sort field
    pub(super) sort_field: SortField,
    /// Current sort direction
    pub(super) sort_direction: SortDirection,
    /// Agent filter (None = show all)
    pub(super) agent_filter: Option<String>,
    /// Search filter - matches filename (case-insensitive)
    pub(super) search_filter: Option<String>,
    /// List state for ratatui
    pub(super) list_state: ListState,
    /// Page size for page up/down navigation
    pub(super) page_size: usize,
}

impl Default for FileExplorer {
    fn default() -> Self {
        Self::new(vec![])
    }
}

impl FileExplorer {
    /// Create a new file explorer with the given items
    pub fn new(items: Vec<FileItem>) -> Self {
        let len = items.len();
        let visible_indices: Vec<usize> = (0..len).collect();

        let mut explorer = Self {
            items,
            visible_indices,
            selected: 0,
            multi_selected: HashSet::new(),
            sort_field: SortField::default(),
            sort_direction: SortDirection::default(),
            agent_filter: None,
            search_filter: None,
            list_state: ListState::default(),
            page_size: 10,
        };

        // Apply initial sort
        explorer.apply_sort();
        explorer.sync_list_state();

        explorer
    }

    /// Get the number of visible items
    pub fn len(&self) -> usize {
        self.visible_indices.len()
    }

    /// Check if the explorer is empty
    pub fn is_empty(&self) -> bool {
        self.visible_indices.is_empty()
    }

    /// Get the currently selected index (in visible list)
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Get the currently selected item, if any
    pub fn selected_item(&self) -> Option<&FileItem> {
        self.visible_indices
            .get(self.selected)
            .map(|&idx| &self.items[idx])
    }

    /// Get all multi-selected items
    pub fn selected_items(&self) -> Vec<&FileItem> {
        self.multi_selected
            .iter()
            .map(|&idx| &self.items[idx])
            .collect()
    }

    /// Get count of multi-selected items
    pub fn selected_count(&self) -> usize {
        self.multi_selected.len()
    }

    /// Check if an item is multi-selected (by visible index)
    pub fn is_selected(&self, visible_idx: usize) -> bool {
        self.visible_indices
            .get(visible_idx)
            .map(|&idx| self.multi_selected.contains(&idx))
            .unwrap_or(false)
    }

    /// Get the current scroll offset of the list.
    pub fn scroll_offset(&self) -> usize {
        self.list_state.offset()
    }

    /// Set the page size for page navigation
    pub fn set_page_size(&mut self, size: usize) {
        self.page_size = size.max(1);
    }

    /// Sync the ratatui ListState with our selected index
    pub(super) fn sync_list_state(&mut self) {
        if self.visible_indices.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(self.selected));
        }
    }

    // === Navigation ===

    /// Move selection up by one
    pub fn up(&mut self) {
        if self.visible_indices.is_empty() {
            return;
        }
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            // Wrap to end
            self.selected = self.visible_indices.len() - 1;
        }
        self.sync_list_state();
    }

    /// Move selection down by one
    pub fn down(&mut self) {
        if self.visible_indices.is_empty() {
            return;
        }
        if self.selected < self.visible_indices.len() - 1 {
            self.selected += 1;
        } else {
            // Wrap to start
            self.selected = 0;
        }
        self.sync_list_state();
    }

    /// Move selection up by a page
    pub fn page_up(&mut self) {
        if self.visible_indices.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(self.page_size);
        self.sync_list_state();
    }

    /// Move selection down by a page
    pub fn page_down(&mut self) {
        if self.visible_indices.is_empty() {
            return;
        }
        self.selected = (self.selected + self.page_size).min(self.visible_indices.len() - 1);
        self.sync_list_state();
    }

    /// Move selection to the first item
    pub fn home(&mut self) {
        if !self.visible_indices.is_empty() {
            self.selected = 0;
            self.sync_list_state();
        }
    }

    /// Move selection to the last item
    pub fn end(&mut self) {
        if !self.visible_indices.is_empty() {
            self.selected = self.visible_indices.len() - 1;
            self.sync_list_state();
        }
    }

    /// Select a specific visible index (for mouse click).
    /// Returns true if the index was valid and selection changed.
    pub fn select_index(&mut self, visible_idx: usize) -> bool {
        if visible_idx < self.visible_indices.len() {
            self.selected = visible_idx;
            self.sync_list_state();
            true
        } else {
            false
        }
    }

    // === Multi-select ===

    /// Toggle selection of the current item
    pub fn toggle_select(&mut self) {
        if let Some(&idx) = self.visible_indices.get(self.selected) {
            if self.multi_selected.contains(&idx) {
                self.multi_selected.remove(&idx);
            } else {
                self.multi_selected.insert(idx);
            }
        }
    }

    /// Select all visible items (skips locked items)
    pub fn select_all(&mut self) {
        for &idx in &self.visible_indices {
            if self.items[idx].lock_info.is_none() {
                self.multi_selected.insert(idx);
            }
        }
    }

    /// Deselect all items
    pub fn select_none(&mut self) {
        self.multi_selected.clear();
    }

    /// Toggle between select all and select none
    pub fn toggle_all(&mut self) {
        let unlocked_count = self
            .visible_indices
            .iter()
            .filter(|&&idx| self.items[idx].lock_info.is_none())
            .count();
        if self.multi_selected.len() >= unlocked_count {
            self.select_none();
        } else {
            self.select_all();
        }
    }
}
