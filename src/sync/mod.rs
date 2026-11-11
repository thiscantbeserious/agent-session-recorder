//! Encrypted session sync.

mod app;
mod backend;
mod crypto;
mod engine;
mod pack;
mod store;

pub use app::{auto, confirm, format, key, setup};
pub use backend::gist::scratch::Scratch;
pub use backend::gist::ProgressSink;
pub use backend::{BackendError, BlobSource, RemoteEntry, SyncBackend};
pub use crypto::{new_writer_id, Key, PhraseError};
pub use engine::lock::SyncLock;
pub use engine::observe::{Event, Observer};
pub use engine::pack_run::Engine;
pub use engine::plan::{Decision, SkipReason};
pub use engine::run::{BackendEngine, Item, Options, Outcome, Report, SyncError};
pub use engine::select::{select_remote, SelectError};
pub use pack::migrate::{
    preview as migrate_preview, run as migrate_run, Env as MigrateEnv, MigrateError,
};

/// Internals for the integration tests, not a stable API.
#[doc(hidden)]
pub mod testing {
    pub use crate::sync::backend::file::FileBackend;
    pub use crate::sync::backend::gist::GistBackend;
    pub use crate::sync::backend::memory::MemoryBackend;
    pub use crate::sync::backend::{pull_bytes, push_bytes};
    pub use crate::sync::crypto::blob::{open_stream, seal_stream, BlobError};
    pub use crate::sync::crypto::CryptoError;
    pub use crate::sync::crypto::{open_meta, seal, seal_meta, sha256_hex, Meta};
    pub use crate::sync::engine::fail::backend_error;
    pub use crate::sync::engine::index;
    pub use crate::sync::engine::plan::{decide, resolve, Command, Input, Local, Remote};
    pub use crate::sync::engine::queue;
    pub use crate::sync::engine::scan::{scan, Row};
    pub use crate::sync::pack::index::{
        new_pack_uuid, open_index, seal_index, Entry, Index, IndexError,
    };
    pub use crate::sync::pack::merge::{merge, PackIn, Rejected, Top};
    pub use crate::sync::pack::migrate::{
        convert_file, convert_index, preview as migrate_preview, recognise, run as migrate_run,
        Env as MigrateEnv, Kind, LegacyGist, LegacyRead, MigrateError, Report as MigrateReport,
    };
    pub use crate::sync::pack::repo::{
        read_pack, OpenRef, Outcome, PushError, PushItem, PushReport, ReadError, Repo, RepoError,
    };
    pub use crate::sync::pack::rules::{
        pack_size, place, should_seal, should_seal_index, valid_path, PackSizeError, Place,
    };
    pub use crate::sync::pack::stream::{open_range, segment, RangeError};
    pub use crate::sync::store::dir::DirStore;
    pub use crate::sync::store::gist::pace::{Clock, ClockError};
    pub use crate::sync::store::gist::runner::{spawn, watch_child, Output, ProcessRunner, Runner};
    pub use crate::sync::store::gist::{pack_dir, GistStore};
    pub use crate::sync::store::memory::MemoryStore;
    pub use crate::sync::store::{Listing, PackInfo, PackStore, Segment, StoreError};

    /// A second handle on the same key bytes, as another machine of the same account holds it.
    pub fn copy_key(key: &crate::sync::Key) -> crate::sync::Key {
        crate::sync::Key::from_bytes(*key.as_bytes())
    }

    /// Gist backend internals for the fake runner tests.
    pub mod gist {
        pub use crate::sync::backend::gist::parse::{parse_list, NO_CREATED_AT};
        pub use crate::sync::backend::gist::parts::{part_name, part_names};
        pub use crate::sync::backend::gist::{MANIFEST_FILE, STUB_BODY, STUB_FILE};
        pub use crate::sync::store::gist::pace::{Clock, FakeClock};
        pub use crate::sync::store::gist::runner::{Output, Runner};
        pub use crate::sync::store::gist::scratch::Scratch;
    }

    /// Blob internals for the framing, nonce and known-answer tests.
    pub mod blob {
        pub use crate::sync::crypto::blob::{
            aad, open_blob, seal_blob, seal_with_prefix, sealed_len, Digest, Opener, Sealer,
            Stream, FRAME, HEADER_LEN, PREFIX_LEN, TAG_LEN,
        };
    }

    /// Key access the tests need, kept off the public `Key` API.
    pub fn key_from_bytes(bytes: [u8; 32]) -> crate::sync::Key {
        crate::sync::Key::from_bytes(bytes)
    }

    pub fn key_bytes(key: &crate::sync::Key) -> &[u8; 32] {
        key.as_bytes()
    }

    pub fn key_cipher(key: &crate::sync::Key) -> chacha20poly1305::XChaCha20Poly1305 {
        key.cipher()
    }
}
