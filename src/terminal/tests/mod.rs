//! Terminal emulator tests.
//!
//! Organized by handler category:
//! - cursor_tests: Cursor movement and positioning
//! - scroll_tests: Scroll region behavior
//! - editing_tests: Erase/delete operations
//! - style_tests: SGR color/attribute parsing
//! - integration_tests: Full sequence replay and fixtures

mod cursor_tests;
mod editing_tests;
mod integration_tests;
mod scroll_tests;
mod style_tests;
