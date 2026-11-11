//! Integration tests for AGR library modules

#[macro_use]
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

#[path = "integration/dependency_features_test.rs"]
mod dependency_features_test;

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

#[path = "integration/sync_engine_test.rs"]
mod sync_engine_test;

#[path = "integration/sync_engine_pack_stamp_test.rs"]
mod sync_engine_pack_stamp_test;
#[path = "integration/sync_engine_pack_test.rs"]
mod sync_engine_pack_test;

#[path = "integration/sync_engine_multi_test.rs"]
mod sync_engine_multi_test;

#[path = "integration/sync_status_format_test.rs"]
mod sync_status_format_test;

#[path = "integration/sync_output_test.rs"]
mod sync_output_test;

#[path = "integration/sync_setup_test.rs"]
mod sync_setup_test;

#[path = "integration/sync_auto_child_test.rs"]
mod sync_auto_child_test;

#[path = "integration/sync_plan_test.rs"]
mod sync_plan_test;

#[path = "integration/sync_index_fields_test.rs"]
mod sync_index_fields_test;

#[path = "integration/cli_sync_parse_test.rs"]
mod cli_sync_parse_test;

#[path = "integration/cli_sync_delete_test.rs"]
mod cli_sync_delete_test;

#[path = "integration/cli_sync_push_pull_test.rs"]
mod cli_sync_push_pull_test;

#[path = "integration/cli_sync_status_test.rs"]
mod cli_sync_status_test;

#[path = "integration/cli_sync_test.rs"]
mod cli_sync_test;

#[path = "integration/sync_multi_machine_test.rs"]
mod sync_multi_machine_test;

#[path = "integration/sync_queue_test.rs"]
mod sync_queue_test;

#[path = "integration/sync_pack_merge_test.rs"]
mod sync_pack_merge_test;

#[path = "integration/sync_pack_stream_test.rs"]
mod sync_pack_stream_test;

#[path = "integration/sync_runner_test.rs"]
mod sync_runner_test;

#[path = "integration/sync_pack_repo_test.rs"]
mod sync_pack_repo_test;

#[path = "integration/sync_pack_repo_cap_test.rs"]
mod sync_pack_repo_cap_test;

#[path = "integration/sync_pack_repo_dir_test.rs"]
mod sync_pack_repo_dir_test;

#[path = "integration/sync_pack_repo_fits_test.rs"]
mod sync_pack_repo_fits_test;

#[path = "integration/sync_pack_migrate_test.rs"]
mod sync_pack_migrate_test;

#[path = "integration/sync_pack_migrate_moving_test.rs"]
mod sync_pack_migrate_moving_test;

#[path = "integration/sync_pack_migrate_report_test.rs"]
mod sync_pack_migrate_report_test;

#[path = "integration/sync_pack_repo_legacy_test.rs"]
mod sync_pack_repo_legacy_test;

#[path = "integration/sync_pack_repo_read_test.rs"]
mod sync_pack_repo_read_test;

#[path = "integration/sync_pack_repo_report_test.rs"]
mod sync_pack_repo_report_test;

#[path = "integration/sync_pack_repo_retry_test.rs"]
mod sync_pack_repo_retry_test;

#[path = "integration/sync_pack_repo_session_test.rs"]
mod sync_pack_repo_session_test;

#[path = "integration/sync_store_contract_test.rs"]
mod sync_store_contract_test;

#[path = "integration/sync_store_gist_test.rs"]
mod sync_store_gist_test;

#[path = "integration/sync_store_gist_append_test.rs"]
mod sync_store_gist_append_test;

#[path = "integration/sync_store_gist_cache_test.rs"]
mod sync_store_gist_cache_test;

#[path = "integration/analyze_options_test.rs"]
mod analyze_options_test;

#[path = "integration/analyze_rename_test.rs"]
mod analyze_rename_test;

#[path = "integration/config_migrate_sections_test.rs"]
mod config_migrate_sections_test;

#[path = "integration/sync_blob_kat_test.rs"]
mod sync_blob_kat_test;

#[path = "integration/sync_blob_stream_test.rs"]
mod sync_blob_stream_test;

#[path = "integration/sync_blob_test.rs"]
mod sync_blob_test;

#[path = "integration/sync_gist_backend_test.rs"]
mod sync_gist_backend_test;

#[path = "integration/sync_gist_legacy_test.rs"]
mod sync_gist_legacy_test;

#[path = "integration/sync_gist_pull_test.rs"]
mod sync_gist_pull_test;

#[path = "integration/sync_gist_push_test.rs"]
mod sync_gist_push_test;

#[path = "integration/sync_scan_path_test.rs"]
mod sync_scan_path_test;

#[path = "integration/tui_cleanup_app_test.rs"]
mod tui_cleanup_app_test;

#[path = "integration/tui_list_app_render_test.rs"]
mod tui_list_app_render_test;

#[path = "integration/tui_list_app_test.rs"]
mod tui_list_app_test;

#[path = "integration/sync_confirm_test.rs"]
mod sync_confirm_test;

#[path = "integration/sync_select_test.rs"]
mod sync_select_test;
