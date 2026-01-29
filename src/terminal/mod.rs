//! Virtual terminal emulator module.
//!
//! Provides a VTE-based terminal buffer for replaying asciicast output.
//! Handles ANSI escape sequences and maintains terminal state.
//!
//! This module is designed as a general-purpose VT emulator that can be used
//! by the player, TUI widgets, and future analysis features.

mod types;

// Stage 2: Types extracted to types.rs
// Stage 3: TerminalBuffer will be moved here
// Stage 4: TerminalPerformer will be moved to performer.rs

pub use types::{Cell, CellStyle, Color, StyledLine};
