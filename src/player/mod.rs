//! Native asciicast player module
//!
//! Provides functionality for playing back asciicast recordings:
//!
//! - `native`: Full-featured native player (seeking, markers, viewport scrolling)
//! - `asciinema`: Legacy wrapper for shelling out to asciinema CLI
//!
//! # Usage
//!
//! ```no_run
//! use agr::player::{play_session, PlaybackResult};
//! use std::path::Path;
//!
//! let result = play_session(Path::new("session.cast")).unwrap();
//! match result {
//!     PlaybackResult::Success(name) => println!("Finished: {}", name),
//!     PlaybackResult::Interrupted => println!("Stopped by user"),
//!     PlaybackResult::Error(e) => eprintln!("Error: {}", e),
//! }
//! ```

mod asciinema;
mod native;

pub use asciinema::{play_session_asciinema, play_session_with_speed};
pub use native::{play_session, play_session_native, PlaybackResult};
