//! Integration tests for AGR library modules

#[path = "integration/helpers/mod.rs"]
pub mod helpers;

#[path = "integration/shell/mod.rs"]
mod shell;

#[path = "integration/asciicast_test.rs"]
mod asciicast_test;

#[path = "integration/backup_gzip_test.rs"]
mod backup_gzip_test;

#[path = "integration/branding_test.rs"]
mod branding_test;

#[path = "integration/cleaner_test.rs"]
mod cleaner_test;

#[path = "integration/codec_size_test.rs"]
mod codec_size_test;
#[path = "integration/codec_test.rs"]
mod codec_test;

#[path = "integration/config_migrate_edge_test.rs"]
mod config_migrate_edge_test;
#[path = "integration/config_migrate_test.rs"]
mod config_migrate_test;
#[path = "integration/config_prompts_test.rs"]
mod config_prompts_test;

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

#[path = "integration/storage_list_test.rs"]
mod storage_list_test;

#[path = "integration/storage_stats_test.rs"]
mod storage_stats_test;

#[path = "integration/storage_lookup_test.rs"]
mod storage_lookup_test;
#[path = "integration/storage_orphans_test.rs"]
mod storage_orphans_test;

#[path = "integration/storage_import_security_test.rs"]
mod storage_import_security_test;
#[path = "integration/storage_import_test.rs"]
mod storage_import_test;

#[path = "integration/storage_import_gzip_test.rs"]
mod storage_import_gzip_test;

#[path = "integration/snapshot_tui_explorer_test.rs"]
mod snapshot_tui_explorer_test;
#[path = "integration/snapshot_tui_import_test.rs"]
mod snapshot_tui_import_test;
#[path = "integration/snapshot_tui_modals_test.rs"]
mod snapshot_tui_modals_test;
#[path = "integration/snapshot_tui_test.rs"]
mod snapshot_tui_test;

#[path = "integration/snapshot_cli_test.rs"]
mod snapshot_cli_test;

#[path = "integration/snapshot_completions_test.rs"]
mod snapshot_completions_test;

#[path = "integration/play_test.rs"]
mod play_test;

#[path = "integration/cli_gzip_test.rs"]
mod cli_gzip_test;

#[path = "integration/preview_test.rs"]
mod preview_test;

#[path = "integration/snapshot_terminal_test.rs"]
mod snapshot_terminal_test;

#[path = "integration/transform_test.rs"]
mod transform_test;

#[path = "integration/transform_roundtrip_test.rs"]
mod transform_roundtrip_test;

#[path = "integration/transform_lock_test.rs"]
mod transform_lock_test;

#[path = "integration/transform_ops_test.rs"]
mod transform_ops_test;

#[path = "integration/transform_ops_backup_test.rs"]
mod transform_ops_backup_test;

#[path = "integration/transform_ops_cleanup_test.rs"]
mod transform_ops_cleanup_test;

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

#[path = "integration/clipboard_gzip_test.rs"]
mod clipboard_gzip_test;

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

#[path = "integration/import_paste_test.rs"]
mod import_paste_test;
#[path = "integration/import_test.rs"]
mod import_test;

#[path = "integration/terminal_transform_test.rs"]
mod terminal_transform_test;

#[path = "integration/dedupe_transform_test.rs"]
mod dedupe_transform_test;

#[path = "integration/content_cleaner_test.rs"]
mod content_cleaner_test;

#[path = "integration/aggressive_similarity_test.rs"]
mod aggressive_similarity_test;
#[path = "integration/aggressive_transform_test.rs"]
mod aggressive_transform_test;

#[path = "integration/analyzer_service_test.rs"]
mod analyzer_service_test;

#[path = "integration/player_loop_test.rs"]
mod player_loop_test;

#[path = "integration/cleanup_draw_test.rs"]
mod cleanup_draw_test;

#[path = "integration/migrate_test.rs"]
mod migrate_test;

#[path = "integration/cli_parse_more_test.rs"]
mod cli_parse_more_test;
#[path = "integration/cli_parse_test.rs"]
mod cli_parse_test;

#[path = "integration/compress_test.rs"]
mod compress_test;

#[path = "integration/cli_compress_test.rs"]
mod cli_compress_test;

#[path = "integration/cli_offer_test.rs"]
mod cli_offer_test;

// CLI colour decision
#[path = "integration/cli_color_test.rs"]
mod cli_color_test;
