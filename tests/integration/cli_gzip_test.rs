//! CLI single-file commands accept `.cast.gz` like `.cast`

use super::helpers::storage::create_codec_session;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const CAST: &str =
    "{\"version\":3,\"term\":{\"cols\":80,\"rows\":24}}\n[0.5,\"o\",\"a\"]\n[10.0,\"o\",\"b\"]\n";
const WARNING: &str = "does not have .cast or .cast.gz extension";

fn run_agr(home: &TempDir, args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_agr"))
        .args(args)
        .env("NO_COLOR", "1")
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join(".config"))
        .output()
        .expect("Failed to execute agr");
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.code().unwrap_or(-1),
    )
}

fn gz_session(temp: &TempDir) -> String {
    let path = create_codec_session(temp.path(), "a", "x.cast.gz", CAST);
    path.to_str().unwrap().to_string()
}

fn starts_with_gzip_magic(path: &Path) -> bool {
    std::fs::read(path).unwrap().starts_with(&[0x1f, 0x8b])
}

fn assert_no_warning(args_for: impl Fn(&str) -> Vec<String>) {
    let temp = TempDir::new().unwrap();
    let file = gz_session(&temp);
    let args = args_for(&file);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (_out, stderr, _code) = run_agr(&temp, &args);
    assert!(!stderr.contains(WARNING), "{args:?}: {stderr}");
}

#[test]
fn play_gz_gives_no_extension_warning() {
    assert_no_warning(|f| vec!["play".into(), f.into()]);
}

#[test]
fn optimize_gz_gives_no_extension_warning() {
    assert_no_warning(|f| vec!["optimize".into(), "--remove-silence".into(), f.into()]);
}

#[test]
fn marker_list_gz_gives_no_extension_warning() {
    assert_no_warning(|f| vec!["marker".into(), "list".into(), f.into()]);
}

#[test]
fn copy_gz_gives_no_extension_warning() {
    assert_no_warning(|f| vec!["copy".into(), f.into()]);
}

#[test]
fn optimize_in_place_keeps_gzip() {
    let temp = TempDir::new().unwrap();
    let file = gz_session(&temp);
    let (_out, stderr, code) = run_agr(&temp, &["optimize", "--remove-silence", &file]);
    assert_eq!(code, 0, "{stderr}");
    assert!(starts_with_gzip_magic(Path::new(&file)));
}

#[test]
fn optimize_output_cast_writes_plain() {
    let temp = TempDir::new().unwrap();
    let file = gz_session(&temp);
    let out = temp.path().join("y.cast");
    let args = ["optimize", "--remove-silence", "--output"];
    let (_out, stderr, code) = run_agr(
        &temp,
        &[args[0], args[1], args[2], out.to_str().unwrap(), &file],
    );
    assert_eq!(code, 0, "{stderr}");
    assert!(!starts_with_gzip_magic(&out));
    assert!(std::fs::read_to_string(&out).unwrap().starts_with('{'));
}
