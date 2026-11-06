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

#[cfg(test)]
mod layer_check {
    use std::path::Path;

    /// (layer, forbidden text). `Command::new` is checked separately.
    const RULES: &[(&str, &str)] = &[
        ("backend/", "crate::sync::crypto"),
        ("backend/", "crate::sync::engine"),
        ("backend/", "crate::sync::app"),
        ("engine/", "crate::sync::app"),
        ("engine/", "crate::sync::backend::gist"),
        ("engine/", "crate::sync::backend::memory"),
        ("engine/", "crate::sync::backend::file"),
        ("app/", "chacha20poly1305"),
        ("engine/", "chacha20poly1305"),
        ("backend/", "chacha20poly1305"),
        ("", "super::super"),
    ];
    const SPAWNERS: &[&str] = &["backend/gist/runner.rs", "app/auto/mod.rs"];

    fn rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                rs_files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }

    #[test]
    fn layer_rules_hold() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sync");
        let mut files = Vec::new();
        rs_files(&root, &mut files);
        let mut bad = Vec::new();
        for f in files.iter().filter(|f| *f != &root.join("mod.rs")) {
            let rel = f
                .strip_prefix(&root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            let text = std::fs::read_to_string(f).unwrap();
            for (layer, needle) in RULES.iter().filter(|(l, _)| rel.starts_with(l)) {
                if text.contains(needle) {
                    bad.push(format!("{rel} ({layer}) must not contain `{needle}`"));
                }
            }
            if text.contains("Command::new") && !SPAWNERS.contains(&rel.as_str()) {
                bad.push(format!("{rel} must not contain `Command::new`"));
            }
            if rel.starts_with("crypto/")
                && text.contains("crate::sync::")
                && !text.contains("crate::sync::crypto")
            {
                bad.push(format!("{rel} (crypto) must not use other sync layers"));
            }
        }
        assert!(bad.is_empty(), "layer rules broken:\n{}", bad.join("\n"));
    }
}
