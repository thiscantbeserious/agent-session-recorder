//! Unit tests for the file explorer.

mod explorer;
mod rendering;

use super::*;
use chrono::{Local, TimeZone};

pub(super) fn create_test_items() -> Vec<FileItem> {
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
