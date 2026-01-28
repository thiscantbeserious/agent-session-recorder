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

pub mod marker;
mod reader;
mod transform;
mod types;
mod writer;

// Re-export marker types
pub use marker::{MarkerInfo, MarkerManager};

// Re-export transform types
pub use transform::{Transform, TransformChain};

// Re-export core types
pub use types::{AsciicastFile, EnvInfo, Event, EventType, Header, TermInfo};
