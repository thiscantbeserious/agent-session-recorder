//! Virtual terminal buffer for replaying asciicast output.
//!
//! This module re-exports from `crate::terminal` for backward compatibility.
//! All types and implementations have been moved to `src/terminal/`.
//! Tests have been migrated to `src/terminal/tests/`.

// Re-export all types from the new terminal module for backward compatibility
pub use crate::terminal::{Cell, CellStyle, Color, StyledLine, TerminalBuffer};
