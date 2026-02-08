//! Agent Session Recorder (AGR) Library

pub mod analyzer;

pub mod asciicast;
pub mod cli;
pub mod theme;

pub mod clipboard;
pub mod config;
pub mod files;
pub mod player;
pub mod recording;
pub mod shell;
pub mod storage;
pub mod terminal;
pub mod tui;
pub mod utils;

pub use asciicast::{AsciicastFile, Event, EventType, Header, MarkerInfo, MarkerManager};
pub use config::Config;
pub use player::{play_session, PlaybackResult};
pub use recording::Recorder;
pub use shell::ShellStatus;
pub use storage::StorageManager;
pub use terminal::TerminalBuffer;
