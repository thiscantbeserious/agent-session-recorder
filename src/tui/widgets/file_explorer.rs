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

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, Local};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Widget},
};

use crate::asciicast::EventType;
use crate::files::backup::has_backup;
use crate::files::lock::{self, LockInfo};
use crate::storage::SessionInfo;
use crate::theme::current_theme;

/// A file item in the explorer
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileItem {
    /// Full path to the file
    pub path: String,
    /// Display name (filename without path)
    pub name: String,
    /// Agent name (e.g., "claude", "codex")
    pub agent: String,
    /// File size in bytes
    pub size: u64,
    /// Last modified time
    pub modified: DateTime<Local>,
    /// Whether a backup file exists for this item (cached)
    pub has_backup: bool,
    /// Lock info if file is actively being recorded (cached)
    pub lock_info: Option<LockInfo>,
}

impl FileItem {
    /// Create a new FileItem
    pub fn new(
        path: impl Into<String>,
        name: impl Into<String>,
        agent: impl Into<String>,
        size: u64,
        modified: DateTime<Local>,
    ) -> Self {
        let path_str = path.into();
        let has_backup = has_backup(std::path::Path::new(&path_str));
        let lock_info = lock::read_lock(std::path::Path::new(&path_str));
        Self {
            path: path_str,
            name: name.into(),
            agent: agent.into(),
            size,
            modified,
            has_backup,
            lock_info,
        }
    }
}

impl From<SessionInfo> for FileItem {
    fn from(session: SessionInfo) -> Self {
        let path_str = session.path.to_string_lossy().to_string();
        let has_backup = has_backup(std::path::Path::new(&path_str));
        let lock_info = lock::read_lock(std::path::Path::new(&path_str));
        Self {
            path: path_str,
            name: session.filename,
            agent: session.agent,
            size: session.size,
            modified: session.modified,
            has_backup,
            lock_info,
        }
    }
}

use crate::terminal::{Color, StyledLine};

/// Enhanced preview information for a session file.
///
/// This data is loaded lazily when a file is selected for preview.
#[derive(Debug, Clone)]
pub struct SessionPreview {
    /// Total duration of the recording in seconds
    pub duration_secs: f64,
    /// Number of marker events
    pub marker_count: usize,
    /// Terminal snapshot at 10% of recording (with color info)
    pub styled_preview: Vec<StyledLine>,
}

impl SessionPreview {
    /// Load preview information from an asciicast file using streaming parsing.
    ///
    /// This is optimized to avoid loading the entire file into memory:
    /// - Parses header for terminal size
    /// - Streams events, only processing terminal output for first ~10%
    /// - Counts markers and sums times for full duration
    ///
    /// Returns None if the file cannot be parsed.
    pub fn load<P: AsRef<Path>>(path: P) -> Option<Self> {
        Self::load_streaming(path)
    }

    /// Streaming loader that minimizes memory usage and processing time.
    ///
    /// Single-pass approach:
    /// - Process terminal output for first ~30 seconds (enough for preview)
    /// - Continue scanning for total duration and marker count
    /// - Never stores all events in memory
    fn load_streaming<P: AsRef<Path>>(path: P) -> Option<Self> {
        use crate::asciicast::{EventType, Header};
        use crate::terminal::TerminalBuffer;
        use std::fs::File;
        use std::io::{BufRead, BufReader};

        let file = File::open(path.as_ref()).ok()?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Parse header
        let header_line = lines.next()?.ok()?;
        let header: Header = serde_json::from_str(&header_line).ok()?;
        if header.version != 3 {
            return None;
        }

        // Get terminal size
        let cols = header.term.as_ref().and_then(|t| t.cols).unwrap_or(80) as usize;
        let rows = header.term.as_ref().and_then(|t| t.rows).unwrap_or(24) as usize;

        let mut buffer = TerminalBuffer::new(cols, rows);
        let mut total_duration = 0.0;
        let mut marker_count = 0;
        let mut preview_captured = false;
        let mut styled_preview = Vec::new();

        // Single pass: process events
        // - Process terminal output until 30 seconds (capture preview snapshot)
        // - Continue counting markers and duration
        const PREVIEW_THRESHOLD_SECS: f64 = 30.0;

        for line_result in lines {
            let line = match line_result {
                Ok(l) if !l.trim().is_empty() => l,
                _ => continue,
            };

            // Quick parse for time, type, and optionally data
            if let Some((time, event_type, data)) = Self::parse_event_minimal(&line) {
                total_duration += time;

                if event_type == EventType::Marker {
                    marker_count += 1;
                }

                // Only process terminal output before threshold
                if !preview_captured {
                    if event_type == EventType::Output {
                        if let Some(output) = data {
                            buffer.process(&output, None);
                        }
                    }

                    // Capture preview at threshold
                    if total_duration >= PREVIEW_THRESHOLD_SECS {
                        styled_preview = buffer.styled_lines();
                        preview_captured = true;
                        // Don't need buffer anymore, drop it
                    }
                }
            }
        }

        // If file was shorter than threshold, capture final state
        if !preview_captured {
            styled_preview = buffer.styled_lines();
        }

        Some(Self {
            duration_secs: total_duration,
            marker_count,
            styled_preview,
        })
    }

    /// Minimal event parsing - only extracts what we need
    fn parse_event_minimal(line: &str) -> Option<(f64, EventType, Option<String>)> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let arr = value.as_array()?;
        if arr.len() < 2 {
            return None;
        }

        let time = arr[0].as_f64()?;
        let type_str = arr[1].as_str()?;
        let event_type = EventType::from_code(type_str)?;

        // Only extract data for output events (avoid string allocation for markers)
        let data = if event_type == EventType::Output && arr.len() >= 3 {
            arr[2].as_str().map(String::from)
        } else {
            None
        };

        Some((time, event_type, data))
    }

    /// Convert our Color enum to ratatui Color
    fn to_ratatui_color(color: Color) -> ratatui::style::Color {
        match color {
            Color::Default => ratatui::style::Color::Reset,
            Color::Black => ratatui::style::Color::Black,
            Color::Red => ratatui::style::Color::Red,
            Color::Green => ratatui::style::Color::Green,
            Color::Yellow => ratatui::style::Color::Yellow,
            Color::Blue => ratatui::style::Color::Blue,
            Color::Magenta => ratatui::style::Color::Magenta,
            Color::Cyan => ratatui::style::Color::Cyan,
            Color::White => ratatui::style::Color::White,
            Color::BrightBlack => ratatui::style::Color::DarkGray,
            Color::BrightRed => ratatui::style::Color::LightRed,
            Color::BrightGreen => ratatui::style::Color::LightGreen,
            Color::BrightYellow => ratatui::style::Color::LightYellow,
            Color::BrightBlue => ratatui::style::Color::LightBlue,
            Color::BrightMagenta => ratatui::style::Color::LightMagenta,
            Color::BrightCyan => ratatui::style::Color::LightCyan,
            Color::BrightWhite => ratatui::style::Color::White,
            Color::Indexed(idx) => ratatui::style::Color::Indexed(idx),
            Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
        }
    }

    /// Convert a StyledLine to a ratatui Line with colors
    pub fn styled_line_to_ratatui(line: &StyledLine) -> Line<'static> {
        // Group consecutive cells with same style into spans
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut current_text = String::new();
        let mut current_style: Option<crate::terminal::CellStyle> = None;

        for cell in &line.cells {
            if Some(cell.style) == current_style {
                current_text.push(cell.char);
            } else {
                // Flush previous span
                if !current_text.is_empty() {
                    if let Some(style) = current_style {
                        let mut ratatui_style = Style::default();
                        if style.fg != Color::Default {
                            ratatui_style = ratatui_style.fg(Self::to_ratatui_color(style.fg));
                        }
                        if style.bg != Color::Default {
                            ratatui_style = ratatui_style.bg(Self::to_ratatui_color(style.bg));
                        }
                        if style.bold {
                            ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                        }
                        if style.dim {
                            ratatui_style = ratatui_style.add_modifier(Modifier::DIM);
                        }
                        if style.italic {
                            ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                        }
                        if style.underline {
                            ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
                        }
                        spans.push(Span::styled(
                            std::mem::take(&mut current_text),
                            ratatui_style,
                        ));
                    } else {
                        spans.push(Span::raw(std::mem::take(&mut current_text)));
                    }
                }
                current_style = Some(cell.style);
                current_text.push(cell.char);
            }
        }

        // Flush final span
        if !current_text.is_empty() {
            if let Some(style) = current_style {
                let mut ratatui_style = Style::default();
                if style.fg != Color::Default {
                    ratatui_style = ratatui_style.fg(Self::to_ratatui_color(style.fg));
                }
                if style.bg != Color::Default {
                    ratatui_style = ratatui_style.bg(Self::to_ratatui_color(style.bg));
                }
                if style.bold {
                    ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                }
                if style.dim {
                    ratatui_style = ratatui_style.add_modifier(Modifier::DIM);
                }
                if style.italic {
                    ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                }
                if style.underline {
                    ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
                }
                spans.push(Span::styled(current_text, ratatui_style));
            } else {
                spans.push(Span::raw(current_text));
            }
        }

        Line::from(spans)
    }

    /// Format duration as human-readable string (e.g., "5m 32s").
    pub fn format_duration(&self) -> String {
        let total_secs = self.duration_secs as u64;
        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;

        if hours > 0 {
            format!("{}h {}m {}s", hours, minutes, seconds)
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds)
        } else {
            format!("{}s", seconds)
        }
    }
}

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
    items: Vec<FileItem>,
    /// Filtered and sorted items (indices into `items`)
    visible_indices: Vec<usize>,
    /// Currently selected index (in visible_indices)
    selected: usize,
    /// Set of selected indices for multi-select (indices into `items`)
    multi_selected: HashSet<usize>,
    /// Current sort field
    sort_field: SortField,
    /// Current sort direction
    sort_direction: SortDirection,
    /// Agent filter (None = show all)
    agent_filter: Option<String>,
    /// Search filter - matches filename (case-insensitive)
    search_filter: Option<String>,
    /// List state for ratatui
    list_state: ListState,
    /// Page size for page up/down navigation
    page_size: usize,
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

    /// Set the page size for page navigation
    pub fn set_page_size(&mut self, size: usize) {
        self.page_size = size.max(1);
    }

    /// Sync the ratatui ListState with our selected index
    fn sync_list_state(&mut self) {
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

    /// Select all visible items
    pub fn select_all(&mut self) {
        for &idx in &self.visible_indices {
            self.multi_selected.insert(idx);
        }
    }

    /// Deselect all items
    pub fn select_none(&mut self) {
        self.multi_selected.clear();
    }

    /// Toggle between select all and select none
    pub fn toggle_all(&mut self) {
        if self.multi_selected.len() == self.visible_indices.len() {
            self.select_none();
        } else {
            self.select_all();
        }
    }

    // === Sorting ===

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

    /// Apply current sort to visible indices
    fn apply_sort(&mut self) {
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
    fn apply_filter(&mut self) {
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
            true
        } else {
            false
        }
    }

    /// Refresh lock_info for visible items that currently have a lock.
    ///
    /// Called periodically to detect when recordings end.
    pub fn refresh_visible_locks(&mut self) {
        for &item_idx in &self.visible_indices {
            if self.items[item_idx].lock_info.is_some() {
                let path = self.items[item_idx].path.clone();
                self.items[item_idx].lock_info = lock::read_lock(std::path::Path::new(&path));
            }
        }
    }

    /// Merge fresh items into the explorer, adding new ones and removing stale ones.
    ///
    /// Preserves the current selection by path if possible.
    /// Re-applies current filters and sort order after merge.
    pub fn merge_items(&mut self, fresh_items: Vec<FileItem>) {
        let selected_path = self.selected_item().map(|i| i.path.clone());

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

        // Clear multi-select (indices are invalidated)
        self.multi_selected.clear();

        // Rebuild filters + sort
        self.apply_filter();
        self.apply_sort();

        // Restore selection by path
        self.restore_selection_by_path(selected_path.as_deref());
        self.sync_list_state();
    }

    /// Restore the selected index to the item with the given path.
    fn restore_selection_by_path(&mut self, path: Option<&str>) {
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

/// Stateless widget for rendering the file explorer
pub struct FileExplorerWidget<'a> {
    explorer: &'a mut FileExplorer,
    show_preview: bool,
    show_checkboxes: bool,
    /// Optional enhanced preview data for the selected session
    session_preview: Option<&'a SessionPreview>,
    /// Whether a backup exists for the selected file
    has_backup: bool,
}

impl<'a> FileExplorerWidget<'a> {
    /// Create a new file explorer widget
    pub fn new(explorer: &'a mut FileExplorer) -> Self {
        Self {
            explorer,
            show_preview: true,
            show_checkboxes: true,
            session_preview: None,
            has_backup: false,
        }
    }

    /// Show or hide the preview panel
    pub fn show_preview(mut self, show: bool) -> Self {
        self.show_preview = show;
        self
    }

    /// Show or hide checkboxes for multi-select
    pub fn show_checkboxes(mut self, show: bool) -> Self {
        self.show_checkboxes = show;
        self
    }

    /// Set the session preview data to display enhanced information
    pub fn session_preview(mut self, preview: Option<&'a SessionPreview>) -> Self {
        self.session_preview = preview;
        self
    }

    /// Set whether a backup exists for the selected file
    pub fn has_backup(mut self, has_backup: bool) -> Self {
        self.has_backup = has_backup;
        self
    }
}

impl Widget for FileExplorerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = current_theme();

        // Layout: list on left, preview on right (if enabled)
        let chunks = if self.show_preview && area.width >= 60 {
            Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area)
        } else {
            Layout::horizontal([Constraint::Percentage(100)]).split(area)
        };

        // Build list items (collect data first to avoid borrow issues)
        // Note: has_backup and lock_info are cached in FileItem
        let item_data: Vec<(String, String, String, bool, bool, bool)> = self
            .explorer
            .visible_items()
            .map(|(_, item, is_checked)| {
                (
                    item.name.clone(),
                    item.agent.clone(),
                    format_size(item.size),
                    is_checked,
                    item.has_backup,
                    item.lock_info.is_some(),
                )
            })
            .collect();

        let show_checkboxes = self.show_checkboxes;
        let items: Vec<ListItem> = item_data
            .iter()
            .map(|(name, agent, size_str, is_checked, has_bak, is_locked)| {
                let mut spans = vec![];
                if show_checkboxes {
                    let checkbox = if *is_checked { "[x] " } else { "[ ] " };
                    spans.push(Span::styled(checkbox, theme.text_secondary_style()));
                }

                // Add [opt] indicator prefix if backup exists
                if *has_bak {
                    spans.push(Span::styled("[opt] ", theme.accent_style()));
                }

                // Use greyed style for locked (actively recording) files
                let name_style = if *is_locked {
                    theme.text_secondary_style()
                } else {
                    theme.text_style()
                };
                spans.push(Span::styled(name.as_str(), name_style));

                // Add recording indicator for locked files
                if *is_locked {
                    spans.push(Span::styled(" \u{1F4F9}", theme.text_secondary_style()));
                }

                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("({}, {})", agent, size_str),
                    theme.text_secondary_style(),
                ));
                ListItem::new(Line::from(spans))
            })
            .collect();

        // Get preview data before mutable borrow
        let preview_data = if self.show_preview && chunks.len() > 1 {
            self.explorer.selected_item().map(|item| {
                (
                    item.name.clone(),
                    item.agent.clone(),
                    item.size,
                    item.modified,
                    item.path.clone(),
                    item.lock_info.clone(),
                )
            })
        } else {
            None
        };

        // Clone session preview data to avoid lifetime issues
        let session_preview_data = self.session_preview.map(|p| {
            (
                p.format_duration(),
                p.marker_count,
                p.styled_preview.clone(),
            )
        });

        // Capture backup status
        let has_backup = self.has_backup;

        // Render list
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Sessions ")
                    .border_style(theme.text_secondary_style()),
            )
            .highlight_style(
                Style::default()
                    .bg(theme.accent)
                    .fg(ratatui::style::Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        // Render with state (mutable borrow here)
        ratatui::widgets::StatefulWidget::render(list, chunks[0], buf, self.explorer.list_state());

        // Render preview panel if enabled
        if self.show_preview && chunks.len() > 1 {
            let preview_text =
                if let Some((name, agent, size, modified, path, lock_data)) = preview_data {
                    let mut lines = vec![
                        Line::from(vec![
                            Span::styled("Name: ", theme.text_secondary_style()),
                            Span::styled(name, theme.text_style()),
                        ]),
                        Line::from(vec![
                            Span::styled("Agent: ", theme.text_secondary_style()),
                            Span::styled(agent, theme.accent_style()),
                        ]),
                        Line::from(vec![
                            Span::styled("Size: ", theme.text_secondary_style()),
                            Span::styled(format_size(size), theme.text_style()),
                        ]),
                    ];

                    // Add duration and markers if session preview is available
                    if let Some((duration, markers, styled_preview)) = session_preview_data {
                        lines.push(Line::from(vec![
                            Span::styled("Duration: ", theme.text_secondary_style()),
                            Span::styled(duration, theme.text_style()),
                        ]));
                        lines.push(Line::from(vec![
                            Span::styled("Markers: ", theme.text_secondary_style()),
                            Span::styled(markers.to_string(), theme.text_style()),
                        ]));
                        // Show backup status
                        if has_backup {
                            lines.push(Line::from(vec![
                                Span::styled("Backup: ", theme.text_secondary_style()),
                                Span::styled(
                                    "Available",
                                    Style::default()
                                        .fg(theme.success)
                                        .add_modifier(Modifier::BOLD),
                                ),
                            ]));
                        }
                        // Show lock/recording status
                        if let Some(ref lock) = lock_data {
                            lines.push(Line::from(vec![
                                Span::styled("Status: ", theme.text_secondary_style()),
                                Span::styled(
                                    format!("\u{1F4F9} Recording (PID {})", lock.pid),
                                    theme.text_secondary_style(),
                                ),
                            ]));
                        }
                        lines.push(Line::from(vec![
                            Span::styled("Modified: ", theme.text_secondary_style()),
                            Span::styled(
                                modified.format("%Y-%m-%d %H:%M").to_string(),
                                theme.text_style(),
                            ),
                        ]));

                        // Add terminal preview section if not empty
                        if !styled_preview.is_empty() {
                            lines.push(Line::from("")); // Empty line separator
                            lines.push(Line::from(vec![Span::styled(
                                "Preview",
                                theme.text_secondary_style(),
                            )]));

                            // Add terminal preview lines with colors (limited to fit)
                            for styled_line in styled_preview.iter().take(12) {
                                // Prepend a space and convert to ratatui Line with colors
                                let mut ratatui_line =
                                    SessionPreview::styled_line_to_ratatui(styled_line);
                                // Insert space at start
                                if let Some(first_span) = ratatui_line.spans.first_mut() {
                                    *first_span = Span::styled(
                                        format!(" {}", first_span.content),
                                        first_span.style,
                                    );
                                } else {
                                    ratatui_line.spans.insert(0, Span::raw(" "));
                                }
                                lines.push(ratatui_line);
                            }
                        }
                    } else {
                        // Fallback when no session preview is available
                        lines.push(Line::from(vec![
                            Span::styled("Modified: ", theme.text_secondary_style()),
                            Span::styled(
                                modified.format("%Y-%m-%d %H:%M").to_string(),
                                theme.text_style(),
                            ),
                        ]));
                        lines.push(Line::from(vec![
                            Span::styled("Path: ", theme.text_secondary_style()),
                            Span::styled(path, theme.text_secondary_style()),
                        ]));
                    }

                    lines
                } else {
                    vec![Line::from("No file selected")]
                };

            let preview = Paragraph::new(preview_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Preview ")
                    .border_style(theme.text_secondary_style()),
            );

            preview.render(chunks[1], buf);
        }
    }
}

/// Format a byte size as human-readable string
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn create_test_items() -> Vec<FileItem> {
        vec![
            FileItem::new(
                "/sessions/claude/session1.cast",
                "session1.cast",
                "claude",
                1024,
                Local.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            ),
            FileItem::new(
                "/sessions/codex/session2.cast",
                "session2.cast",
                "codex",
                2048,
                Local.with_ymd_and_hms(2024, 1, 16, 11, 0, 0).unwrap(),
            ),
            FileItem::new(
                "/sessions/claude/session3.cast",
                "session3.cast",
                "claude",
                512,
                Local.with_ymd_and_hms(2024, 1, 14, 9, 0, 0).unwrap(),
            ),
        ]
    }

    #[test]
    fn new_explorer_has_all_items_visible() {
        let explorer = FileExplorer::new(create_test_items());
        assert_eq!(explorer.len(), 3);
        assert!(!explorer.is_empty());
    }

    #[test]
    fn empty_explorer_is_empty() {
        let explorer = FileExplorer::new(vec![]);
        assert!(explorer.is_empty());
        assert_eq!(explorer.len(), 0);
    }

    #[test]
    fn selected_starts_at_zero() {
        let explorer = FileExplorer::new(create_test_items());
        assert_eq!(explorer.selected(), 0);
    }

    #[test]
    fn down_moves_selection() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.down();
        assert_eq!(explorer.selected(), 1);
        explorer.down();
        assert_eq!(explorer.selected(), 2);
    }

    #[test]
    fn down_wraps_to_start() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.down();
        explorer.down();
        explorer.down(); // Should wrap
        assert_eq!(explorer.selected(), 0);
    }

    #[test]
    fn up_moves_selection() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.end(); // Go to last
        explorer.up();
        assert_eq!(explorer.selected(), 1);
    }

    #[test]
    fn up_wraps_to_end() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.up(); // Should wrap to end
        assert_eq!(explorer.selected(), 2);
    }

    #[test]
    fn home_goes_to_first() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.down();
        explorer.down();
        explorer.home();
        assert_eq!(explorer.selected(), 0);
    }

    #[test]
    fn end_goes_to_last() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.end();
        assert_eq!(explorer.selected(), 2);
    }

    #[test]
    fn page_down_moves_by_page_size() {
        let items: Vec<FileItem> = (0..20)
            .map(|i| {
                FileItem::new(
                    format!("/path/{}.cast", i),
                    format!("{}.cast", i),
                    "test",
                    100,
                    Local::now(),
                )
            })
            .collect();
        let mut explorer = FileExplorer::new(items);
        explorer.set_page_size(5);
        explorer.page_down();
        assert_eq!(explorer.selected(), 5);
        explorer.page_down();
        assert_eq!(explorer.selected(), 10);
    }

    #[test]
    fn page_up_moves_by_page_size() {
        let items: Vec<FileItem> = (0..20)
            .map(|i| {
                FileItem::new(
                    format!("/path/{}.cast", i),
                    format!("{}.cast", i),
                    "test",
                    100,
                    Local::now(),
                )
            })
            .collect();
        let mut explorer = FileExplorer::new(items);
        explorer.set_page_size(5);
        explorer.end(); // Go to 19
        explorer.page_up();
        assert_eq!(explorer.selected(), 14);
    }

    #[test]
    fn toggle_select_adds_and_removes() {
        let mut explorer = FileExplorer::new(create_test_items());
        assert_eq!(explorer.selected_count(), 0);

        explorer.toggle_select();
        assert_eq!(explorer.selected_count(), 1);
        assert!(explorer.is_selected(0));

        explorer.toggle_select();
        assert_eq!(explorer.selected_count(), 0);
        assert!(!explorer.is_selected(0));
    }

    #[test]
    fn select_all_selects_all_visible() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.select_all();
        assert_eq!(explorer.selected_count(), 3);
    }

    #[test]
    fn select_none_clears_selection() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.select_all();
        explorer.select_none();
        assert_eq!(explorer.selected_count(), 0);
    }

    #[test]
    fn toggle_all_toggles_between_all_and_none() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.toggle_all(); // Select all
        assert_eq!(explorer.selected_count(), 3);
        explorer.toggle_all(); // Deselect all
        assert_eq!(explorer.selected_count(), 0);
    }

    #[test]
    fn default_sort_is_date_descending() {
        let explorer = FileExplorer::new(create_test_items());
        assert_eq!(explorer.sort_field(), SortField::Date);
        assert_eq!(explorer.sort_direction(), SortDirection::Descending);
        // Newest first (2024-01-16 is index 1, but after sort it should be first)
        let first = explorer.selected_item().unwrap();
        assert_eq!(first.name, "session2.cast"); // Jan 16 is newest
    }

    #[test]
    fn set_sort_changes_field() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_sort(SortField::Name);
        assert_eq!(explorer.sort_field(), SortField::Name);
        // After name sort descending, session3 should be first
        let first = explorer.selected_item().unwrap();
        assert_eq!(first.name, "session3.cast");
    }

    #[test]
    fn set_sort_same_field_toggles_direction() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_sort(SortField::Name);
        assert_eq!(explorer.sort_direction(), SortDirection::Descending);
        explorer.set_sort(SortField::Name);
        assert_eq!(explorer.sort_direction(), SortDirection::Ascending);
    }

    #[test]
    fn filter_by_agent_shows_only_matching() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_agent_filter(Some("claude".to_string()));
        assert_eq!(explorer.len(), 2);
        for (_, item, _) in explorer.visible_items() {
            assert_eq!(item.agent, "claude");
        }
    }

    #[test]
    fn filter_none_shows_all() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_agent_filter(Some("claude".to_string()));
        explorer.set_agent_filter(None);
        assert_eq!(explorer.len(), 3);
    }

    #[test]
    fn unique_agents_returns_sorted_list() {
        let explorer = FileExplorer::new(create_test_items());
        let agents = explorer.unique_agents();
        assert_eq!(agents, vec!["claude", "codex"]);
    }

    #[test]
    fn selected_item_returns_correct_item() {
        let explorer = FileExplorer::new(create_test_items());
        let item = explorer.selected_item().unwrap();
        // Default sort is date descending, so newest (session2) is first
        assert_eq!(item.name, "session2.cast");
    }

    #[test]
    fn selected_items_returns_multi_selected() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.toggle_select(); // Select first visible
        explorer.down();
        explorer.toggle_select(); // Select second visible
        let items = explorer.selected_items();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn navigation_on_empty_explorer_does_not_panic() {
        let mut explorer = FileExplorer::new(vec![]);
        explorer.up();
        explorer.down();
        explorer.page_up();
        explorer.page_down();
        explorer.home();
        explorer.end();
        explorer.toggle_select();
        // No panic means success
    }

    #[test]
    fn format_size_works() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1048576), "1.0 MB");
        assert_eq!(format_size(1073741824), "1.0 GB");
    }

    // Search filter tests

    #[test]
    fn search_filter_matches_filename_case_insensitive() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_search_filter(Some("session1".to_string()));
        assert_eq!(explorer.len(), 1);
        assert_eq!(explorer.selected_item().unwrap().name, "session1.cast");
    }

    #[test]
    fn search_filter_case_insensitive() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_search_filter(Some("SESSION1".to_string()));
        assert_eq!(explorer.len(), 1);
        assert_eq!(explorer.selected_item().unwrap().name, "session1.cast");
    }

    #[test]
    fn search_filter_partial_match() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_search_filter(Some("session".to_string()));
        assert_eq!(explorer.len(), 3); // All match "session"
    }

    #[test]
    fn search_filter_no_match_returns_empty() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_search_filter(Some("nonexistent".to_string()));
        assert_eq!(explorer.len(), 0);
        assert!(explorer.selected_item().is_none());
    }

    #[test]
    fn search_filter_none_shows_all() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_search_filter(Some("session1".to_string()));
        assert_eq!(explorer.len(), 1);
        explorer.set_search_filter(None);
        assert_eq!(explorer.len(), 3);
    }

    #[test]
    fn search_and_agent_filters_combine() {
        let mut explorer = FileExplorer::new(create_test_items());
        // Filter to claude only
        explorer.set_agent_filter(Some("claude".to_string()));
        assert_eq!(explorer.len(), 2);
        // Then search within claude sessions
        explorer.set_search_filter(Some("session1".to_string()));
        assert_eq!(explorer.len(), 1);
        assert_eq!(explorer.selected_item().unwrap().name, "session1.cast");
        assert_eq!(explorer.selected_item().unwrap().agent, "claude");
    }

    #[test]
    fn clear_filters_clears_both_search_and_agent() {
        let mut explorer = FileExplorer::new(create_test_items());
        explorer.set_agent_filter(Some("claude".to_string()));
        explorer.set_search_filter(Some("session1".to_string()));
        assert_eq!(explorer.len(), 1);
        explorer.clear_filters();
        assert_eq!(explorer.len(), 3);
        assert!(explorer.agent_filter().is_none());
        assert!(explorer.search_filter().is_none());
    }

    #[test]
    fn search_filter_getter_returns_value() {
        let mut explorer = FileExplorer::new(create_test_items());
        assert!(explorer.search_filter().is_none());
        explorer.set_search_filter(Some("test".to_string()));
        assert_eq!(explorer.search_filter(), Some("test"));
    }

    // From<SessionInfo> conversion test
    #[test]
    fn file_item_from_session_info() {
        use std::path::PathBuf;

        let session = SessionInfo {
            path: PathBuf::from("/sessions/claude/test.cast"),
            agent: "claude".to_string(),
            filename: "test.cast".to_string(),
            size: 1024,
            modified: Local.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap(),
            age_days: 0,
            age_hours: 0,
            age_minutes: 0,
        };

        let item = FileItem::from(session);
        assert_eq!(item.path, "/sessions/claude/test.cast");
        assert_eq!(item.name, "test.cast");
        assert_eq!(item.agent, "claude");
        assert_eq!(item.size, 1024);
    }

    // SessionPreview tests

    #[test]
    fn session_preview_format_duration_seconds() {
        let preview = SessionPreview {
            duration_secs: 45.0,
            marker_count: 0,
            styled_preview: Vec::new(),
        };
        assert_eq!(preview.format_duration(), "45s");
    }

    #[test]
    fn session_preview_format_duration_minutes() {
        let preview = SessionPreview {
            duration_secs: 332.0, // 5m 32s
            marker_count: 0,
            styled_preview: Vec::new(),
        };
        assert_eq!(preview.format_duration(), "5m 32s");
    }

    #[test]
    fn session_preview_format_duration_hours() {
        let preview = SessionPreview {
            duration_secs: 3732.0, // 1h 2m 12s
            marker_count: 0,
            styled_preview: Vec::new(),
        };
        assert_eq!(preview.format_duration(), "1h 2m 12s");
    }
}
