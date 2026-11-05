//! Explorer state, filter, sort and metadata tests.

use super::create_test_items;
use crate::files::lock;
use crate::storage::SessionInfo;
use crate::tui::widgets::file_explorer::render_helpers::format_size;
use crate::tui::widgets::file_explorer::*;
use chrono::{Local, TimeZone};
use std::fs;
use tempfile::TempDir;

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

#[test]
fn refresh_visible_item_metadata_updates_size_when_lock_appears_after_creation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("session.cast");
    fs::write(&path, b"abc").unwrap();
    let modified = Local.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();

    // Item is created before lock exists, so cached lock_info starts as None.
    let item = FileItem::new(
        path.to_string_lossy().as_ref(),
        "session.cast",
        "claude",
        3,
        modified,
    );
    let mut explorer = FileExplorer::new(vec![item]);
    assert!(explorer.selected_item().unwrap().lock_info.is_none());

    // Recording lock appears and file grows.
    lock::create_lock(&path).unwrap();
    fs::write(&path, b"abcdefghij").unwrap();

    explorer.refresh_visible_item_metadata();
    let selected = explorer.selected_item().unwrap();
    assert_eq!(selected.size, 10);
    assert!(selected.lock_info.is_some());

    lock::remove_lock(&path);
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
