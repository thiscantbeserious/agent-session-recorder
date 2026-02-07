//! Preview prefetching helpers for TUI explorer applications.
//!
//! Provides `prefetch_adjacent_previews` which both `list_app` and
//! `cleanup_app` use to prefetch session previews for the current,
//! previous, and next items in the file explorer.

/// Prefetch previews for the current, previous, and next items.
///
/// Collects up to 3 paths (current selection, previous with wrap,
/// next with wrap) and submits them to the cache for background loading.
/// Extracted from `list_app.rs` and `cleanup_app.rs` which had identical logic.
pub fn prefetch_adjacent_previews(
    explorer: &super::FileExplorer,
    cache: &mut crate::tui::lru_cache::PreviewCache,
) {
    let selected = explorer.selected();
    let len = explorer.len();
    if len == 0 {
        return;
    }

    // Collect paths to prefetch (current, prev, next)
    let mut paths_to_prefetch = Vec::with_capacity(3);

    // Current selection
    if let Some(item) = explorer.selected_item() {
        paths_to_prefetch.push(item.path.clone());
    }

    // Previous item (with wrap)
    let prev_idx = if selected > 0 { selected - 1 } else { len - 1 };
    if let Some((_, item, _)) = explorer.visible_items().nth(prev_idx) {
        paths_to_prefetch.push(item.path.clone());
    }

    // Next item (with wrap)
    let next_idx = if selected < len - 1 { selected + 1 } else { 0 };
    if let Some((_, item, _)) = explorer.visible_items().nth(next_idx) {
        paths_to_prefetch.push(item.path.clone());
    }

    // Request prefetch for all
    cache.prefetch(&paths_to_prefetch);
}
