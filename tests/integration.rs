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

#[path = "integration/tui_test.rs"]
mod tui_test;

#[path = "integration/cli_test.rs"]
mod cli_test;
