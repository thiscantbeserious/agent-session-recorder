//! Compress core: per-file migration, failure injection, plan, run and estimate

use super::helpers::storage::{create_codec_session, create_test_config};
use agr::files::{backup, codec, lock};
use agr::storage::compress::{
    compress_file, compress_file_with_verify, estimate, plan, run, Outcome,
};
use agr::StorageManager;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const CAST: &str = "{\"version\":3,\"width\":80,\"height\":24}\n[0.1,\"o\",\"x\"]\n";

fn manager(temp: &TempDir) -> StorageManager {
    StorageManager::new(create_test_config(temp))
}

fn plain(temp: &TempDir, agent: &str, name: &str) -> PathBuf {
    create_codec_session(temp.path(), agent, name, CAST)
}

fn gz(path: &Path) -> PathBuf {
    path.with_extension("cast.gz")
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn compress_file_leaves_only_gzip_with_equal_content() {
    let temp = TempDir::new().unwrap();
    let path = plain(&temp, "a", "x.cast");
    let original = fs::read(&path).unwrap();
    let outcome = compress_file(&path).unwrap();
    assert!(matches!(outcome, Outcome::Compressed { .. }), "{outcome:?}");
    assert_eq!(files_in(path.parent().unwrap()), vec!["x.cast.gz"]);
    let mut decoded = Vec::new();
    std::io::Read::read_to_end(&mut codec::open_reader(&gz(&path)).unwrap(), &mut decoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn verify_mismatch_keeps_plain_and_removes_gzip_and_temp() {
    let temp = TempDir::new().unwrap();
    let path = plain(&temp, "a", "x.cast");
    let result = compress_file_with_verify(&path, |_, _| Ok(false));
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), CAST);
    assert_eq!(files_in(path.parent().unwrap()), vec!["x.cast"]);
}

#[cfg(unix)]
#[test]
fn read_only_dir_keeps_plain_untouched() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let path = plain(&temp, "a", "x.cast");
    let dir = path.parent().unwrap();
    fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
    let result = compress_file(&path);
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert_eq!(files_in(dir), vec!["x.cast"]);
    assert_eq!(fs::read_to_string(&path).unwrap(), CAST);
}

#[test]
fn backup_becomes_gzip_backup_with_equal_content() {
    let temp = TempDir::new().unwrap();
    let path = plain(&temp, "a", "x.cast");
    backup::create_backup(&path).unwrap();
    compress_file(&path).unwrap();
    let new_backup = backup::backup_path_for(&gz(&path));
    assert_eq!(
        files_in(path.parent().unwrap()),
        vec![".x.cast.gz.bak", "x.cast.gz"]
    );
    let mut decoded = String::new();
    let file = fs::File::open(&new_backup).unwrap();
    std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(file), &mut decoded).unwrap();
    assert_eq!(decoded, CAST);
}

#[test]
fn plan_skips_live_lock_and_cleans_stale_lock() {
    let temp = TempDir::new().unwrap();
    let live = plain(&temp, "a", "live.cast");
    let stale = plain(&temp, "a", "stale.cast");
    lock::create_lock(&live).unwrap();
    let stale_lock = lock::lock_path_for(&stale);
    fs::write(
        &stale_lock,
        r#"{"pid":4294967,"started":"2020-01-01T00:00:00Z"}"#,
    )
    .unwrap();
    let plan = plan(&manager(&temp), None).unwrap();
    assert_eq!(plan.live, vec![live]);
    let eligible: Vec<_> = plan.files.iter().map(|f| f.path.clone()).collect();
    assert_eq!(eligible, vec![stale]);
    assert!(!stale_lock.exists());
}

#[test]
fn plan_with_agent_filter_only_includes_that_agent() {
    let temp = TempDir::new().unwrap();
    plain(&temp, "a", "x.cast");
    let b = plain(&temp, "b", "y.cast");
    let plan = plan(&manager(&temp), Some("b")).unwrap();
    assert_eq!(plan.files.len(), 1);
    assert_eq!(plan.files[0].path, b);
    assert_eq!(plan.files[0].agent, "b");
}

#[test]
fn run_continues_after_failure_and_counts() {
    let temp = TempDir::new().unwrap();
    let bad = plain(&temp, "a", "bad.cast");
    let good = plain(&temp, "a", "good.cast");
    let live = plain(&temp, "a", "live.cast");
    lock::create_lock(&live).unwrap();
    let mut p = plan(&manager(&temp), None).unwrap();
    fs::remove_file(&bad).unwrap();
    p.files.sort_by_key(|f| f.path != bad);
    let report = run(&p);
    assert_eq!(report.compressed, 1);
    assert_eq!(report.failed.len(), 1);
    assert_eq!(report.failed[0].path, bad);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].path, live);
    assert!(gz(&good).exists());
    assert!(report.bytes_saved > 0 || fs::metadata(gz(&good)).unwrap().len() >= CAST.len() as u64);
}

#[test]
fn second_run_finds_nothing_to_do() {
    let temp = TempDir::new().unwrap();
    plain(&temp, "a", "x.cast");
    let m = manager(&temp);
    let first = run(&plan(&m, None).unwrap());
    assert_eq!(first.compressed, 1);
    let again = plan(&m, None).unwrap();
    assert!(again.files.is_empty() && again.live.is_empty());
}

#[test]
fn leftover_plain_next_to_equal_gzip_is_removed() {
    let temp = TempDir::new().unwrap();
    let path = plain(&temp, "a", "x.cast");
    codec::transcode(&path, &gz(&path)).unwrap();
    let report = run(&plan(&manager(&temp), None).unwrap());
    assert_eq!(report.compressed, 1);
    assert_eq!(files_in(path.parent().unwrap()), vec!["x.cast.gz"]);
}

#[test]
fn leftover_plain_next_to_differing_gzip_is_skipped_with_reason() {
    let temp = TempDir::new().unwrap();
    let path = plain(&temp, "a", "x.cast");
    create_codec_session(temp.path(), "a", "x.cast.gz", "different\n");
    let report = run(&plan(&manager(&temp), None).unwrap());
    assert_eq!(report.compressed, 0);
    assert_eq!(report.skipped.len(), 1);
    assert!(
        report.skipped[0].reason.contains("differs"),
        "{:?}",
        report.skipped
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), CAST);
}

#[test]
fn estimate_is_within_2x_of_real_size() {
    let temp = TempDir::new().unwrap();
    let body: String = (0..20000)
        .map(|i| format!("[{i}.5,\"o\",\"line {} of output\\r\\n\"]\n", i % 97))
        .collect();
    let path = create_codec_session(temp.path(), "a", "big.cast", &(CAST.to_string() + &body));
    let projected = estimate(&path).unwrap();
    codec::transcode(&path, &gz(&path)).unwrap();
    let real = fs::metadata(gz(&path)).unwrap().len();
    assert!(
        projected * 2 >= real && projected <= real * 2,
        "{projected} vs {real}"
    );
}
