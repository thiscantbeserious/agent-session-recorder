//! `agr update` through the real binary, against a fake release on disk.

use crate::helpers::update_fake::{build, FakeRelease, Spec};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A private copy of the agr binary, so `update` can replace it.
fn copy_agr() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let bin = dir.path().join("agr");
    fs::copy(env!("CARGO_BIN_EXE_agr"), &bin).unwrap();
    (dir, bin)
}

fn run(bin: &PathBuf, base: &str, args: &[&str]) -> Output {
    Command::new(bin)
        .args(args)
        .env("AGR_UPDATE_RELEASE_BASE", base)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

fn spec(version: &str) -> Spec {
    Spec {
        version: version.into(),
        ..Spec::default()
    }
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn listing(dir: &TempDir) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

fn fake(version: &str) -> FakeRelease {
    build(&spec(version))
}

#[test]
fn check_update_available_exits_10() {
    let (dir, bin) = copy_agr();
    let before = listing(&dir);
    let rel = fake("0.5.0");
    let o = run(&bin, &rel.base(), &["update", "--check"]);
    let s = out(&o);
    assert_eq!(o.status.code(), Some(10), "{s}");
    assert!(s.contains(CURRENT) && s.contains("0.5.0"), "{s}");
    assert!(s.contains("update available"), "{s}");
    assert_eq!(listing(&dir), before);
}

#[test]
fn check_up_to_date_exits_0() {
    let (_dir, bin) = copy_agr();
    let rel = fake(CURRENT);
    let o = run(&bin, &rel.base(), &["update", "--check"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).contains("up to date"));
}

#[test]
fn check_dev_build_newer_exits_0() {
    let (_dir, bin) = copy_agr();
    let rel = fake("0.0.1");
    let o = run(&bin, &rel.base(), &["update", "--check"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(out(&o).contains("newer"), "{}", out(&o));
}

#[test]
fn check_without_checksums_notes_it() {
    let (_dir, bin) = copy_agr();
    let rel = build(&Spec {
        version: "9.0.0".into(),
        no_sums: true,
        ..Spec::default()
    });
    let o = run(&bin, &rel.base(), &["update", "--check"]);
    let s = out(&o);
    assert_eq!(o.status.code(), Some(10), "{s}");
    assert!(s.contains("cannot be applied automatically"), "{s}");
    assert!(s.contains("no checksums"), "{s}");
}

#[test]
fn check_failure_exits_1() {
    let (_dir, bin) = copy_agr();
    let empty = TempDir::new().unwrap();
    let base = format!("file://{}", empty.path().display());
    let o = run(&bin, &base, &["update", "--check"]);
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(!err.trim().is_empty(), "no cause printed");
}

#[test]
fn update_up_to_date_changes_nothing() {
    let (_dir, bin) = copy_agr();
    let bytes = fs::read(&bin).unwrap();
    let mtime = fs::metadata(&bin).unwrap().modified().unwrap();
    let rel = fake(CURRENT);
    let o = run(&bin, &rel.base(), &["update"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(fs::read(&bin).unwrap(), bytes);
    assert_eq!(fs::metadata(&bin).unwrap().modified().unwrap(), mtime);
}

#[test]
fn update_success_prints_old_and_new() {
    let (_dir, bin) = copy_agr();
    let rel = fake("9.0.0");
    let o = run(&bin, &rel.base(), &["update"]);
    let s = out(&o);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{s} {}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(s.contains(CURRENT) && s.contains("9.0.0"), "{s}");
    let v = Command::new(&bin).arg("--version").output().unwrap();
    assert!(out(&v).contains("9.0.0"), "{}", out(&v));
}

#[test]
fn update_refuses_release_without_checksums() {
    let (_dir, bin) = copy_agr();
    let before = fs::read(&bin).unwrap();
    let rel = build(&Spec {
        version: "9.0.0".into(),
        no_sums: true,
        ..Spec::default()
    });
    let o = run(&bin, &rel.base(), &["update"]);
    assert_eq!(o.status.code(), Some(1));
    let err = String::from_utf8_lossy(&o.stderr).to_string();
    assert!(err.contains("no checksums"), "{err}");
    assert_eq!(fs::read(&bin).unwrap(), before);
}

#[test]
fn update_never_downloads_on_check() {
    let (_dir, bin) = copy_agr();
    let rel = fake("9.0.0");
    fs::remove_file(rel.asset_path()).unwrap();
    fs::create_dir(rel.asset_path()).unwrap();
    let o = run(&bin, &rel.base(), &["update", "--check"]);
    assert_eq!(o.status.code(), Some(10));
}

#[test]
fn cli_help_update() {
    let o = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(["update", "--help"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let s = out(&o);
    assert_eq!(o.status.code(), Some(0), "{s}");
    for word in ["--check", "10", "curl", "Windows"] {
        assert!(s.contains(word), "missing {word}: {s}");
    }
    assert!(!s.contains("AGR_UPDATE_RELEASE_BASE"));
    insta::assert_snapshot!("cli_help_update", s);
}

#[test]
fn main_help_lists_update() {
    let o = Command::new(env!("CARGO_BIN_EXE_agr"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(out(&o).contains("update"));
}
