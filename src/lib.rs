//! Agent Session Recorder (ASR) Library
//!
//! A Rust library for recording AI agent terminal sessions with asciinema.

pub mod analyzer;
pub mod asciicast;
pub mod branding;
pub mod cli;
pub mod config;
pub mod player;
pub mod recording;
pub mod shell;
pub mod storage;
pub mod terminal;
pub mod tui;

pub use analyzer::Analyzer;
pub use asciicast::{AsciicastFile, Event, EventType, Header, MarkerInfo, MarkerManager};
pub use config::Config;
pub use player::{play_session, PlaybackResult};
pub use recording::Recorder;
pub use shell::ShellStatus;
pub use storage::StorageManager;
pub use terminal::TerminalBuffer;
