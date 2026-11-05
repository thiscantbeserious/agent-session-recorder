//! Storage tests: gzip-aware import

use super::helpers::storage::{compression_config, create_codec_session};
use agr::config::Compression;
use agr::files::codec;
use agr::storage::{validate_cast_header, ImportError};
use agr::StorageManager;
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const CAST: &str = "{\"version\":3,\"width\":80,\"height\":24}\n[0.1,\"o\",\"x\"]\n";

fn import_manager(temp: &TempDir, c: Compression) -> StorageManager {
    StorageManager::new(compression_config(temp, c))
}

fn import_ok(m: &StorageManager, src: &Path) -> PathBuf {
    let result = m.import_cast_file(src, "claude");
    assert!(result.is_ok(), "{result:?}");
    result.unwrap()
}

#[test]
fn import_gz_with_valid_content_succeeds() {
    let temp = TempDir::new().unwrap();
    let src = create_codec_session(temp.path(), "src", "a.cast.gz", CAST);
    let out = import_ok(&import_manager(&temp, Compression::Gzip), &src);
    assert!(out.ends_with("claude/a.cast.gz"));
    assert_eq!(codec::open_reader(&out).unwrap().lines().count(), 2);
}

#[test]
fn import_gz_with_non_asciicast_content_is_invalid_format() {
    let temp = TempDir::new().unwrap();
    let src = create_codec_session(temp.path(), "src", "a.cast.gz", "not json\n");
    let err = import_manager(&temp, Compression::Gzip)
        .import_cast_file(&src, "claude")
        .unwrap_err();
    assert!(matches!(err, ImportError::InvalidFormat(_)), "{err}");
}

#[test]
fn import_plain_under_gzip_config_lands_as_gz() {
    let temp = TempDir::new().unwrap();
    let src = create_codec_session(temp.path(), "src", "a.cast", CAST);
    let out = import_ok(&import_manager(&temp, Compression::Gzip), &src);
    assert!(out.ends_with("claude/a.cast.gz"), "{}", out.display());
    assert!(codec::same_content(&src, &out).unwrap());
}

#[test]
fn import_gz_under_none_config_lands_as_plain() {
    let temp = TempDir::new().unwrap();
    let src = create_codec_session(temp.path(), "src", "a.cast.gz", CAST);
    let out = import_ok(&import_manager(&temp, Compression::None), &src);
    assert!(out.ends_with("claude/a.cast"), "{}", out.display());
    assert_eq!(fs::read_to_string(&out).unwrap(), CAST);
}

#[test]
fn import_conflict_suffixing_works_on_gz() {
    let temp = TempDir::new().unwrap();
    let src = create_codec_session(temp.path(), "src", "x.cast.gz", CAST);
    let m = import_manager(&temp, Compression::Gzip);
    import_ok(&m, &src);
    let second = import_ok(&m, &src);
    assert!(
        second.ends_with("claude/x-1.cast.gz"),
        "{}",
        second.display()
    );
}

#[test]
fn import_conflict_checks_both_variants() {
    let temp = TempDir::new().unwrap();
    create_codec_session(temp.path(), "claude", "x.cast", CAST);
    let src = create_codec_session(temp.path(), "src", "x.cast", CAST);
    let out = import_ok(&import_manager(&temp, Compression::Gzip), &src);
    assert!(out.ends_with("claude/x-1.cast.gz"), "{}", out.display());
}

#[test]
fn wrong_extension_message_mentions_gz() {
    let temp = TempDir::new().unwrap();
    let txt = temp.path().join("a.txt");
    fs::write(&txt, CAST).unwrap();
    let err = validate_cast_header(&txt).unwrap_err();
    assert!(
        err.to_string().contains("Expected .cast or .cast.gz"),
        "{err}"
    );
}
