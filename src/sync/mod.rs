//! Encrypted session sync.

mod app;
mod backend;
mod crypto;
mod engine;
mod store;

pub use app::{auto, format, key, setup};
pub use backend::gist::ProgressSink;
pub use backend::{BackendError, BlobSource, RemoteEntry, SyncBackend};
pub use crypto::{Key, PhraseError};
pub use engine::observe::{Event, Observer};
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
    pub use crate::sync::crypto::{open_meta, seal, seal_meta, sha256_hex, Meta};
    pub use crate::sync::engine::index;
    pub use crate::sync::engine::plan::{decide, resolve, Command, Remote};
    pub use crate::sync::engine::queue;
    pub use crate::sync::engine::scan::{scan, Row};
    pub use crate::sync::store::memory::MemoryStore;
    pub use crate::sync::store::{Listing, PackInfo, PackStore, Segment, StoreError};
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
        ("app/", "sha2::"),
        ("engine/", "sha2::"),
        ("backend/", "sha2::"),
        ("app/", "aead::"),
        ("engine/", "aead::"),
        ("backend/", "aead::"),
        ("pack/", "crate::sync::app"),
        ("pack/", "crate::sync::engine"),
        ("pack/", "crate::sync::store::memory"),
        ("pack/", "crate::sync::store::dir"),
        ("pack/", "crate::sync::store::gist"),
        ("store/", "crate::sync::crypto"),
        ("store/", "crate::sync::engine"),
        ("store/", "crate::sync::pack"),
        ("store/", "crate::sync::app"),
        ("store/", "chacha20poly1305"),
        ("store/", "sha2::"),
        ("store/", "aead::"),
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

    /// Expands `a::{b, c::{d}}` into `a::b` and `a::c::d`.
    fn expand(path: &str) -> Vec<String> {
        let Some(open) = path.find('{') else {
            return vec![path.to_string()];
        };
        let (mut depth, mut start, mut parts) = (0, open + 1, Vec::new());
        for (i, c) in path.char_indices().skip(open) {
            match c {
                '{' => depth += 1,
                '}' | ',' if depth == 1 => {
                    parts.push((start, i));
                    start = i + 1;
                    if c == '}' {
                        let rest = &path[i + 1..];
                        return parts
                            .iter()
                            .flat_map(|&(s, e)| {
                                expand(&format!("{}{}{rest}", &path[..open], &path[s..e]))
                            })
                            .collect();
                    }
                }
                '}' => depth -= 1,
                _ => {}
            }
        }
        vec![path.to_string()]
    }

    /// Every path named by a `use` statement, groups expanded.
    fn use_paths(text: &str) -> Vec<String> {
        text.split(';')
            .filter_map(|stmt| stmt.find("use ").map(|i| &stmt[i + 4..]))
            .flat_map(|body| expand(&body.split_whitespace().collect::<String>()))
            .collect()
    }

    /// Drops `//` line comments and `/* */` block comments (not nested, not string aware).
    fn strip_comments(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(i) = rest.find('/') {
            out.push_str(&rest[..i]);
            let tail = &rest[i..];
            rest = if tail.starts_with("//") {
                tail.find('\n').map_or("", |n| &tail[n..])
            } else if tail.starts_with("/*") {
                tail.find("*/").map_or("", |n| &tail[n + 2..])
            } else {
                out.push('/');
                &tail[1..]
            };
        }
        out.push_str(rest);
        out
    }

    /// Rule violations of one file, `rel` is relative to `src/sync`.
    fn violations(rel: &str, text: &str) -> Vec<String> {
        let text = &strip_comments(text);
        let paths = use_paths(text);
        let mut bad = Vec::new();
        for (layer, needle) in RULES.iter().filter(|(l, _)| rel.starts_with(l)) {
            if text.contains(needle) || paths.iter().any(|p| p.contains(needle)) {
                bad.push(format!("{rel} ({layer}) must not contain `{needle}`"));
            }
        }
        if text.contains("Command::new") && !SPAWNERS.contains(&rel) {
            bad.push(format!("{rel} must not contain `Command::new`"));
        }
        let other_layer =
            |p: &String| p.starts_with("crate::sync::") && !p.starts_with("crate::sync::crypto");
        if rel.starts_with("crypto/") && paths.iter().any(other_layer) {
            bad.push(format!("{rel} (crypto) must not use other sync layers"));
        }
        bad
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
            bad.extend(violations(&rel, &std::fs::read_to_string(f).unwrap()));
        }
        assert!(bad.is_empty(), "layer rules broken:\n{}", bad.join("\n"));
    }

    #[test]
    fn violations_flags_plain_import() {
        let v = violations("backend/x.rs", "use crate::sync::crypto::Key;");
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn violations_flags_group_import_in_backend() {
        let v = violations("backend/x.rs", "use crate::sync::{crypto, engine};");
        assert_eq!(v.len(), 2, "{v:?}");
    }

    #[test]
    fn violations_flags_nested_group_import_in_engine() {
        let v = violations(
            "engine/x.rs",
            "use crate::sync::backend::{gist::GistBackend};",
        );
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn violations_flags_crypto_file_that_also_imports_crypto() {
        let src = "use crate::sync::crypto::Key;\nuse crate::sync::engine::index;";
        let v = violations("crypto/x.rs", src);
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn violations_ignores_use_inside_comments() {
        let src = "// we use the engine later\n/* use this */\nuse crate::sync::engine::index;";
        let v = violations("crypto/x.rs", src);
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn violations_flags_sha2_and_aead_outside_crypto() {
        assert_eq!(violations("app/x.rs", "use sha2::Sha256;").len(), 1);
        assert_eq!(violations("engine/x.rs", "use aead::Aead;").len(), 1);
        assert!(violations("crypto/x.rs", "use sha2::Sha256;").is_empty());
    }

    #[test]
    fn violations_allows_sibling_imports_within_layer() {
        let src = "use crate::sync::{engine::index, backend::BlobSource};";
        assert!(violations("engine/x.rs", src).is_empty());
    }

    #[test]
    fn violations_flags_pack_naming_engine() {
        let v = violations("pack/x.rs", "use crate::sync::engine::index;");
        assert_eq!(v.len(), 1, "{v:?}");
    }

    #[test]
    fn violations_flags_store_naming_crypto() {
        let v = violations("store/x.rs", "use crate::sync::crypto::Key;");
        assert_eq!(v.len(), 1, "{v:?}");
    }
}
