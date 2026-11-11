//! Layer rules of `src/sync`, read from the source text. Reads files only, so it needs no facade.

use std::path::Path;

/// (layer, forbidden text). `Command::new` is checked separately.
const RULES: &[(&str, &str)] = &[
    ("", "crate::sync::backend"),
    ("engine/", "crate::sync::app"),
    ("engine/", "crate::sync::crypto"),
    ("engine/", "crate::sync::store"),
    ("app/", "chacha20poly1305"),
    ("engine/", "chacha20poly1305"),
    ("app/", "sha2::"),
    ("engine/", "sha2::"),
    ("app/", "aead::"),
    ("engine/", "aead::"),
    ("pack/", "crate::sync::app"),
    ("pack/", "crate::sync::engine"),
    ("pack/", "crate::sync::store::memory"),
    ("pack/", "crate::sync::store::dir"),
    ("pack/", "crate::sync::store::gist"),
    ("pack/", "chacha20poly1305"),
    ("pack/", "aead::"),
    ("pack/", "sha2::"),
    // the engine owns its index file and its field names
    ("pack/", ".agr-sync.json"),
    ("pack/", "remote_id"),
    ("store/", "crate::sync::crypto"),
    ("store/", "crate::sync::engine"),
    ("store/", "crate::sync::pack"),
    ("store/", "crate::sync::app"),
    ("store/", "chacha20poly1305"),
    ("store/", "sha2::"),
    ("store/", "aead::"),
    ("", "super::super"),
];
/// (path of the needle, the only layers that may name it).
const ONLY_IN: &[(&str, &[&str])] = &[("crate::sync::store::legacy", &["pack/migrate/", "store/"])];
const SPAWNERS: &[&str] = &["store/gist/runner.rs", "app/auto/mod.rs"];

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
/// `needle` occurs and ends on a segment boundary, so `crate::sync::app` does not
/// match `crate::sync::appx`. A needle ending in `::` (`sha2::`) is a prefix.
fn has_segment(text: &str, needle: &str) -> bool {
    if needle.ends_with("::") {
        return text.contains(needle);
    }
    text.match_indices(needle).any(|(i, _)| {
        let next = text[i + needle.len()..].chars().next();
        !next.is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Pack files that are not tests, only tests may name the concrete stores in `testing`.
fn is_pack_production(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    let test = name.starts_with("tests") || name.ends_with("_tests.rs");
    rel.starts_with("pack/") && !test
}

/// Layers that may spell the backend name, `app/setup.rs` has the one config value match.
const GIST_OK: &[&str] = &[
    "store/gist/",
    "store/legacy/",
    "app/setup.rs",
    // user facing wording about secret gists and the old gists migrate deletes
    "app/confirm.rs",
    "app/format/packs.rs",
    "app/format/summary.rs",
    "pack/migrate/mod.rs",
];

/// A string literal with `gist` in it (any case), text after `#[cfg(test)]` is skipped.
fn has_gist_literal(text: &str) -> bool {
    let code = text.split("#[cfg(test)]").next().unwrap_or(text);
    code.split('"')
        .skip(1)
        .step_by(2)
        .any(|lit| lit.to_lowercase().contains("gist"))
}

fn violations(rel: &str, text: &str) -> Vec<String> {
    let text = &strip_comments(text);
    let paths = use_paths(text);
    let mut bad = Vec::new();
    for (layer, needle) in RULES.iter().filter(|(l, _)| rel.starts_with(l)) {
        if has_segment(text, needle) || paths.iter().any(|p| has_segment(p, needle)) {
            bad.push(format!("{rel} ({layer}) must not contain `{needle}`"));
        }
    }
    if is_pack_production(rel) && has_segment(text, "crate::sync::testing") {
        bad.push(format!(
            "{rel} (pack) must not contain `crate::sync::testing`"
        ));
    }
    for (needle, allowed) in ONLY_IN {
        let named = has_segment(text, needle) || paths.iter().any(|p| has_segment(p, needle));
        if named && !allowed.iter().any(|a| rel.starts_with(a)) {
            bad.push(format!("{rel} must not contain `{needle}`"));
        }
    }
    if has_gist_literal(text) && !GIST_OK.iter().any(|a| rel.starts_with(a)) {
        bad.push(format!("{rel} must not hold a `gist` string literal"));
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
    let v = violations("store/x.rs", "use crate::sync::crypto::Key;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_group_import_in_backend() {
    let v = violations("store/x.rs", "use crate::sync::{crypto, engine};");
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
fn violations_match_whole_path_segments_only() {
    // `super::supervise` is not `super::super`, `crate::sync::appx` is not `crate::sync::app`.
    assert!(violations("store/x.rs", "fn f() { super::supervise(); }").is_empty());
    assert!(violations("engine/x.rs", "use crate::sync::appx::Thing;").is_empty());
    assert_eq!(violations("store/x.rs", "use super::super::x;").len(), 1);
}

#[test]
fn violations_allows_sibling_imports_within_layer() {
    let src = "use crate::sync::{engine::index, pack::Key};";
    assert!(violations("engine/x.rs", src).is_empty());
}

#[test]
fn violations_flags_pack_naming_engine() {
    let v = violations("pack/x.rs", "use crate::sync::engine::index;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_pack_naming_chacha20poly1305() {
    let v = violations("pack/x.rs", "use chacha20poly1305::XNonce;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_brace_form_crypto_in_store() {
    let v = violations("store/x.rs", "use crate::sync::{crypto::Key};");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_store_naming_crypto() {
    let v = violations("store/x.rs", "use crate::sync::crypto::Key;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_pack_naming_backend() {
    let v = violations("pack/x.rs", "use crate::sync::backend::BlobSource;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_pack_production_naming_testing() {
    let src = "use crate::sync::testing::MemoryStore;";
    assert_eq!(violations("pack/repo/push.rs", src).len(), 1);
    assert!(violations("pack/repo/tests_dir.rs", src).is_empty());
    assert!(violations("pack/merge_tests.rs", src).is_empty());
    assert!(violations("pack/stream/tests.rs", src).is_empty());
}

#[test]
fn violations_flags_engine_naming_gist_store() {
    let v = violations("engine/x.rs", "use crate::sync::store::gist::GistStore;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_any_backend_path() {
    let v = violations("engine/x.rs", "use crate::sync::backend::X;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_engine_naming_crypto_or_store() {
    let v = violations("engine/x.rs", "use crate::sync::crypto::Key;");
    assert_eq!(v.len(), 1, "{v:?}");
    let v = violations("engine/x.rs", "use crate::sync::store::PackStore;");
    assert_eq!(v.len(), 1, "{v:?}");
}

#[test]
fn violations_flags_legacy_outside_pack_migrate() {
    let v = violations("app/x.rs", "use crate::sync::store::legacy::X;");
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(violations("pack/migrate/x.rs", "use crate::sync::store::legacy::X;").is_empty());
    assert!(violations("store/x.rs", "use crate::sync::store::legacy::X;").is_empty());
}

#[test]
fn violations_flags_gist_literal_outside_gist_store() {
    let v = violations("engine/x.rs", "let n = \"gist\";");
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(violations("store/gist/x.rs", "let n = \"Gist\";").is_empty());
    assert!(violations("store/legacy/x.rs", "let n = \"gist\";").is_empty());
    let test_only = "let n = 1;\n#[cfg(test)]\nmod t { const N: &str = \"gist\"; }";
    assert!(violations("engine/x.rs", test_only).is_empty());
}
