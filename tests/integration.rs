//! Integration tests for AGR library modules

#[path = "integration/helpers/mod.rs"]
pub mod helpers;

#[path = "integration/shell/mod.rs"]
mod shell;

#[path = "integration/asciicast_test.rs"]
mod asciicast_test;

#[path = "integration/branding_test.rs"]
mod branding_test;

#[path = "integration/config_test.rs"]
mod config_test;

#[path = "integration/filename_test.rs"]
mod filename_test;

#[path = "integration/markers_test.rs"]
mod markers_test;

#[path = "integration/recording_test.rs"]
mod recording_test;

#[path = "integration/shell_test.rs"]
mod shell_test;

#[path = "integration/storage_test.rs"]
mod storage_test;

#[path = "integration/snapshot_tui_test.rs"]
mod snapshot_tui_test;

#[path = "integration/snapshot_cli_test.rs"]
mod snapshot_cli_test;

#[path = "integration/snapshot_completions_test.rs"]
mod snapshot_completions_test;

#[path = "integration/play_test.rs"]
mod play_test;

#[path = "integration/preview_test.rs"]
mod preview_test;

#[path = "integration/snapshot_terminal_test.rs"]
mod snapshot_terminal_test;

#[path = "integration/transform_test.rs"]
mod transform_test;

#[path = "integration/terminal_test.rs"]
mod terminal_test;

#[path = "integration/terminal_cursor_test.rs"]
mod terminal_cursor_test;

#[path = "integration/terminal_scroll_test.rs"]
mod terminal_scroll_test;

#[path = "integration/terminal_editing_test.rs"]
mod terminal_editing_test;

#[path = "integration/terminal_style_test.rs"]
mod terminal_style_test;

#[path = "integration/snapshot_player_test.rs"]
mod snapshot_player_test;

#[path = "integration/resize_stress_test.rs"]
mod resize_stress_test;

#[path = "integration/clipboard_test.rs"]
mod clipboard_test;

#[path = "integration/copy_test.rs"]
mod copy_test;

#[path = "integration/analyzer_content_test.rs"]
mod analyzer_content_test;

#[path = "integration/lock_test.rs"]
mod lock_test;

#[path = "integration/file_explorer_test.rs"]
mod file_explorer_test;

#[path = "integration/process_guard_test.rs"]
mod process_guard_test;
