//! Encrypted session sync.

mod app;
mod backend;
mod crypto;
mod engine;

pub use app::{auto, format, key, setup};
pub use backend::{BackendError, BlobSource, RemoteEntry, SyncBackend};
pub use crypto::{Key, PhraseError};
pub use engine::plan::{Decision, SkipReason};
pub use engine::run::{Engine, Item, Options, Outcome, Report, SyncError};
pub use engine::scan::Duplicate;

/// Internals for the integration tests, not a stable API.
#[doc(hidden)]
pub mod testing {
    pub use crate::sync::backend::gist::GistBackend;
    pub use crate::sync::backend::memory::MemoryBackend;
    pub use crate::sync::backend::{pull_bytes, push_bytes};
    pub use crate::sync::crypto::blob::{open_stream, seal_stream, BlobError};
    pub use crate::sync::crypto::{open_meta, seal, seal_meta, sha256_hex, CryptoError, Meta};
    pub use crate::sync::engine::index;
    pub use crate::sync::engine::plan::{decide, resolve, Command, Remote};
    pub use crate::sync::engine::scan::{scan, Row};
}
