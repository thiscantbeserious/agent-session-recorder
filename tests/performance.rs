//! Performance tests for AGR library modules.
//!
//! These tests verify performance requirements and should be run with:
//! `cargo test --test performance`

#[path = "performance/mod.rs"]
pub mod helpers;

#[path = "performance/content_extraction_test.rs"]
mod content_extraction_test;

#[path = "performance/silence_removal_test.rs"]
mod silence_removal_test;

#[path = "performance/transform_cli_test.rs"]
mod transform_cli_test;
