//! Terminal escape sequence handlers.
//!
//! This module contains handlers for various escape sequence categories:
//! - cursor: Cursor movement and positioning
//! - editing: Erase and delete operations
//! - scroll: Scroll region management
//! - style: SGR (Select Graphic Rendition) for colors and attributes

pub mod cursor;
pub mod editing;
pub mod scroll;
pub mod style;

use tracing::trace;

/// Log an unhandled CSI sequence for debugging.
pub fn log_unhandled_csi(action: char, params: &[u16], intermediates: &[u8]) {
    trace!(
        action = %action,
        params = ?params,
        intermediates = ?intermediates,
        "Unhandled CSI sequence"
    );
}

/// Log an unhandled ESC sequence for debugging.
pub fn log_unhandled_esc(byte: u8, intermediates: &[u8]) {
    trace!(
        byte = byte,
        byte_char = %char::from(byte),
        intermediates = ?intermediates,
        "Unhandled ESC sequence"
    );
}
