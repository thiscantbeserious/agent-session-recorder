//! Lock enforcement tests for transform, analyze and marker commands.

use tempfile::TempDir;

use super::transform_test::{create_cast_file, run_agr, sample_cast_with_long_pauses};

// ============================================================================
// Lock Enforcement Tests
// ============================================================================

#[test]
fn transform_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "locked.cast", sample_cast_with_long_pauses());
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked") || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}",
        stderr
    );
}

#[test]
fn transform_cleans_stale_lock_and_proceeds() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "stale.cast", sample_cast_with_long_pauses());
    let lock_path = agr::files::lock::lock_path_for(&cast_path);
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    )
    .unwrap();

    let (_stdout, _stderr, exit_code) =
        run_agr(&["optimize", "--remove-silence", cast_path.to_str().unwrap()]);

    assert_eq!(exit_code, 0, "Should succeed after cleaning stale lock");
    assert!(!lock_path.exists(), "Stale lock should be cleaned up");
}

#[test]
fn analyze_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(
        &temp_dir,
        "locked_analyze.cast",
        sample_cast_with_long_pauses(),
    );
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) = run_agr(&["analyze", cast_path.to_str().unwrap()]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked") || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}",
        stderr
    );
}

#[test]
fn marker_add_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(
        &temp_dir,
        "locked_marker.cast",
        sample_cast_with_long_pauses(),
    );
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) = run_agr(&[
        "marker",
        "add",
        cast_path.to_str().unwrap(),
        "0.5",
        "test marker",
    ]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked") || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}",
        stderr
    );
}

#[test]
fn transform_requires_transform_flag() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "test.cast", sample_cast_with_long_pauses());

    // Run transform without any transform flag
    let (stdout, stderr, exit_code) = run_agr(&["optimize", cast_path.to_str().unwrap()]);

    assert_ne!(exit_code, 0, "Should fail without optimization flag");
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("remove-silence") || combined.contains("No optimization"),
        "Should indicate need for optimization flag. Output: {}",
        combined
    );
}

#[test]
fn remove_silence_on_gzip_equals_the_plain_twin() {
    use std::io::Read;
    let temp_dir = TempDir::new().unwrap();
    let plain = create_cast_file(&temp_dir, "a.cast", sample_cast_with_long_pauses());
    let gz = temp_dir.path().join("b.cast.gz");
    agr::files::codec::transcode(&plain, &gz).unwrap();

    for file in [&plain, &gz] {
        let (_o, e, code) = run_agr(&["optimize", "--remove-silence", file.to_str().unwrap()]);
        assert_eq!(code, 0, "{e}");
    }

    let mut out = String::new();
    agr::files::codec::open_reader(&gz)
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    assert!(std::fs::read(&gz).unwrap().starts_with(&[0x1f, 0x8b]));
    assert_eq!(out, std::fs::read_to_string(&plain).unwrap());
}
