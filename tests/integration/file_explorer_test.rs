//! Integration tests for FileExplorer::merge_items()

use agr::tui::widgets::{FileExplorer, FileItem};
use chrono::{Local, TimeZone};

fn make_item(path: &str, name: &str, agent: &str, size: u64, day: u32) -> FileItem {
    FileItem::new(
        path,
        name,
        agent,
        size,
        Local.with_ymd_and_hms(2024, 1, day, 10, 0, 0).unwrap(),
    )
}

fn base_items() -> Vec<FileItem> {
    vec![
        make_item("/s/claude/a.cast", "a.cast", "claude", 1024, 15),
        make_item("/s/codex/b.cast", "b.cast", "codex", 2048, 16),
        make_item("/s/claude/c.cast", "c.cast", "claude", 512, 14),
    ]
}

#[test]
fn merge_items_adds_new_items() {
    let mut explorer = FileExplorer::new(base_items());
    assert_eq!(explorer.len(), 3);

    let fresh = vec![
        make_item("/s/claude/a.cast", "a.cast", "claude", 1024, 15),
        make_item("/s/codex/b.cast", "b.cast", "codex", 2048, 16),
        make_item("/s/claude/c.cast", "c.cast", "claude", 512, 14),
        make_item("/s/codex/d.cast", "d.cast", "codex", 4096, 17),
    ];

    explorer.merge_items(fresh);
    assert_eq!(explorer.len(), 4);
}

#[test]
fn merge_items_removes_stale_items() {
    let mut explorer = FileExplorer::new(base_items());
    assert_eq!(explorer.len(), 3);

    // Fresh list only has 2 of the 3 original items
    let fresh = vec![
        make_item("/s/claude/a.cast", "a.cast", "claude", 1024, 15),
        make_item("/s/claude/c.cast", "c.cast", "claude", 512, 14),
    ];

    explorer.merge_items(fresh);
    assert_eq!(explorer.len(), 2);

    // Verify the removed item is gone
    let paths: Vec<&str> = explorer
        .visible_items()
        .map(|(_, item, _)| item.path.as_str())
        .collect();
    assert!(!paths.contains(&"/s/codex/b.cast"));
}

#[test]
fn merge_items_preserves_selection_by_path() {
    let mut explorer = FileExplorer::new(base_items());
    // Default sort is by date descending: b(16), a(15), c(14)
    // Select the second visible item (a.cast)
    explorer.down();
    let selected_before = explorer.selected_item().unwrap().path.clone();
    assert_eq!(selected_before, "/s/claude/a.cast");

    // Add a new item that sorts before a.cast
    let fresh = vec![
        make_item("/s/claude/a.cast", "a.cast", "claude", 1024, 15),
        make_item("/s/codex/b.cast", "b.cast", "codex", 2048, 16),
        make_item("/s/claude/c.cast", "c.cast", "claude", 512, 14),
        make_item("/s/codex/d.cast", "d.cast", "codex", 4096, 17),
    ];

    explorer.merge_items(fresh);

    // Selection should still be on a.cast
    let selected_after = explorer.selected_item().unwrap().path.clone();
    assert_eq!(selected_after, "/s/claude/a.cast");
}

#[test]
fn merge_items_handles_empty_fresh_list() {
    let mut explorer = FileExplorer::new(base_items());
    assert_eq!(explorer.len(), 3);

    explorer.merge_items(vec![]);
    assert_eq!(explorer.len(), 0);
    assert!(explorer.is_empty());
}

#[test]
fn merge_items_no_changes_is_noop() {
    let mut explorer = FileExplorer::new(base_items());
    // Default sort: b(16), a(15), c(14). Select b.
    let selected_before = explorer.selected_item().unwrap().path.clone();
    assert_eq!(selected_before, "/s/codex/b.cast");

    // Merge identical items
    explorer.merge_items(base_items());

    assert_eq!(explorer.len(), 3);
    let selected_after = explorer.selected_item().unwrap().path.clone();
    assert_eq!(selected_after, "/s/codex/b.cast");
}
