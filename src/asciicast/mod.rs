//! Asciicast v3 format parser, writer, and transformation utilities.
//!
//! This module provides types and utilities for working with asciicast v3 files,
//! the format used by asciinema for terminal recordings.
//!
//! Reference: <https://docs.asciinema.org/manual/asciicast/v3/>
//!
//! # Module Structure
//!
//! - [`types`] - Core type definitions (Header, Event, AsciicastFile)
//! - [`reader`] - Parsing asciicast files from various sources
//! - [`writer`] - Writing asciicast files to various destinations
//! - [`marker`] - Adding and listing markers in recordings
//! - [`transform`] - Event transformation pipeline utilities
//! - [`transform_ops`] - High-level file transform operations (backup, restore)

pub mod marker;
mod reader;
mod silence_removal;
mod transform;
pub mod transform_ops;
mod types;
mod writer;

// Re-export marker types
pub use marker::{MarkerInfo, MarkerManager};

// Re-export silence removal types
pub use silence_removal::{SilenceRemoval, DEFAULT_SILENCE_THRESHOLD};

// Re-export transform types
pub use transform::{Transform, TransformChain};

// Re-export transform_ops types for convenience
pub use transform_ops::{
    apply_transforms, backup_path_for, has_backup, restore_from_backup, TransformResult,
};

// Re-export core types
pub use types::{AsciicastFile, EnvInfo, Event, EventType, Header, TermInfo};
