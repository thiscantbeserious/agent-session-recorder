//! Agent Session Recorder (ASR) Library
//!
//! A Rust library for recording AI agent terminal sessions with asciinema.

pub mod asciicast;
pub mod config;
pub mod markers;
pub mod recording;
pub mod shell;
pub mod skills;
pub mod storage;

pub use asciicast::{AsciicastFile, Event, EventType, Header};
pub use config::Config;
pub use markers::MarkerManager;
pub use recording::Recorder;
pub use shell::ShellStatus;
pub use storage::StorageManager;
