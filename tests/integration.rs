//! Integration tests for AGR library modules

#[path = "integration/helpers/mod.rs"]
pub mod helpers;

#[path = "integration/analyzer_test.rs"]
mod analyzer_test;

#[path = "integration/asciicast_test.rs"]
mod asciicast_test;

#[path = "integration/branding_test.rs"]
mod branding_test;

#[path = "integration/config_test.rs"]
mod config_test;

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

#[path = "integration/preview_test.rs"]
mod preview_test;
