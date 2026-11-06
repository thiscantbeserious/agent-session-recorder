//! Unit tests for recording module

use agr::{Config, Recorder};

#[test]
fn generate_filename_has_correct_format() {
    let config = Config::default();
    let recorder = Recorder::new(config);
    let filename = recorder.generate_filename();
    assert!(filename.ends_with(".cast"));
    // New format: {directory}_{date}_{time}.cast
    assert!(filename.contains('_'));
}

#[test]
fn sanitize_filename_preserves_valid_chars() {
    assert_eq!(Recorder::sanitize_filename("my-session"), "my-session.cast");
    assert_eq!(Recorder::sanitize_filename("test_123"), "test_123.cast");
    assert_eq!(Recorder::sanitize_filename("file.cast"), "file.cast");
}

#[test]
fn sanitize_filename_replaces_spaces_with_dashes() {
    assert_eq!(Recorder::sanitize_filename("my session"), "my-session.cast");
    assert_eq!(Recorder::sanitize_filename("a b c"), "a-b-c.cast");
}

#[test]
fn sanitize_filename_replaces_special_chars() {
    assert_eq!(Recorder::sanitize_filename("test@#$%"), "test____.cast");
    assert_eq!(Recorder::sanitize_filename("file/name"), "file_name.cast");
}

#[test]
fn sanitize_filename_adds_extension() {
    assert_eq!(Recorder::sanitize_filename("session"), "session.cast");
}

#[test]
fn sanitize_filename_keeps_existing_extension() {
    assert_eq!(Recorder::sanitize_filename("session.cast"), "session.cast");
}

mod finalize {
    use agr::config::Compression;
    use agr::recording::finalize_recording;
    use agr::Config;
    use std::fs;

    const CAST: &str = "{\"version\":3}\n[0.1,\"o\",\"hi\"]\n";

    fn config(compression: Compression) -> Config {
        let mut config = Config::default();
        config.storage.compression = compression;
        config
    }

    #[test]
    fn gzip_config_returns_gz_path_and_removes_plain() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("x.cast");
        fs::write(&plain, CAST).unwrap();
        let result = finalize_recording(&plain, &config(Compression::Gzip));
        assert_eq!(result, dir.path().join("x.cast.gz"));
        assert!(result.exists());
        assert!(!plain.exists());
    }

    #[test]
    fn none_config_returns_plain_path_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("x.cast");
        fs::write(&plain, CAST).unwrap();
        let result = finalize_recording(&plain, &config(Compression::None));
        assert_eq!(result, plain);
        assert_eq!(fs::read_to_string(&plain).unwrap(), CAST);
    }

    #[test]
    fn compress_failure_returns_plain_path_and_keeps_file() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("x.cast");
        fs::write(&plain, CAST).unwrap();
        // A directory in the way makes verification of the leftover gz fail.
        fs::create_dir(dir.path().join("x.cast.gz")).unwrap();
        let result = finalize_recording(&plain, &config(Compression::Gzip));
        assert_eq!(result, plain);
        assert_eq!(fs::read_to_string(&plain).unwrap(), CAST);
    }
}

mod optimize {
    use agr::config::Compression;
    use agr::recording::{finalize_recording, optimize_recording};
    use agr::Config;
    use flate2::read::GzDecoder;
    use std::fs;
    use std::io::Read;
    use std::path::Path;

    const GAP: &str = "{\"version\":3}\n[0.1,\"o\",\"a\"]\n[5.0,\"o\",\"b\"]\n";
    const ZERO_LIMIT: &str =
        "{\"version\":3,\"idle_time_limit\":0}\n[0.1,\"o\",\"a\"]\n[5.0,\"o\",\"b\"]\n";
    const BROKEN: &str = "not a cast\n[5.0,\"o\",\"b\"]\n";

    fn config(optimize: bool, compression: Compression) -> Config {
        let mut config = Config::default();
        config.recording.auto_optimize = optimize;
        config.storage.compression = compression;
        config
    }

    fn setup(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("x.cast");
        fs::write(&plain, content).unwrap();
        (dir, plain)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    fn read_gz(path: &Path) -> String {
        let mut out = String::new();
        GzDecoder::new(fs::File::open(path).unwrap())
            .read_to_string(&mut out)
            .unwrap();
        out
    }

    #[test]
    fn disabled_leaves_file_identical() {
        let (_dir, plain) = setup(GAP);
        assert_eq!(
            optimize_recording(&plain, &config(false, Compression::None)),
            None
        );
        assert_eq!(fs::read_to_string(&plain).unwrap(), GAP);
    }

    #[test]
    fn enabled_clamps_gap_without_backup() {
        let (dir, plain) = setup(GAP);
        assert_eq!(
            optimize_recording(&plain, &config(true, Compression::None)),
            None
        );
        let out = fs::read_to_string(&plain).unwrap();
        assert!(out.contains("[2.0,"), "gap not clamped: {out}");
        assert!(!out.contains("[5.0,"), "gap kept: {out}");
        assert_eq!(names(dir.path()), vec!["x.cast"]);
    }

    #[test]
    fn unparsable_file_warns_and_stays_identical() {
        let (_dir, plain) = setup(BROKEN);
        let warning = optimize_recording(&plain, &config(true, Compression::None));
        assert!(
            warning
                .as_deref()
                .is_some_and(|w| w.starts_with("Auto-optimize failed, kept original: ")),
            "{warning:?}"
        );
        assert_eq!(fs::read_to_string(&plain).unwrap(), BROKEN);
    }

    #[test]
    fn zero_idle_limit_warns_and_stays_identical() {
        let (_dir, plain) = setup(ZERO_LIMIT);
        let warning = optimize_recording(&plain, &config(true, Compression::None));
        assert!(
            warning
                .as_deref()
                .is_some_and(|w| w.starts_with("Auto-optimize failed, ")),
            "{warning:?}"
        );
        assert_eq!(fs::read_to_string(&plain).unwrap(), ZERO_LIMIT);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("x.cast");
        assert_eq!(
            optimize_recording(&missing, &config(true, Compression::None)),
            None
        );
    }

    #[test]
    fn optimize_then_gzip_leaves_only_optimized_gz() {
        let (dir, plain) = setup(GAP);
        let cfg = config(true, Compression::Gzip);
        assert_eq!(optimize_recording(&plain, &cfg), None);
        let result = finalize_recording(&plain, &cfg);
        assert_eq!(names(dir.path()), vec!["x.cast.gz"]);
        let out = read_gz(&result);
        assert!(out.contains("[2.0,") && !out.contains("[5.0,"), "{out}");
    }

    #[test]
    fn optimize_then_none_leaves_only_optimized_plain() {
        let (dir, plain) = setup(GAP);
        let cfg = config(true, Compression::None);
        assert_eq!(optimize_recording(&plain, &cfg), None);
        let result = finalize_recording(&plain, &cfg);
        assert_eq!(names(dir.path()), vec!["x.cast"]);
        let out = fs::read_to_string(result).unwrap();
        assert!(out.contains("[2.0,") && !out.contains("[5.0,"), "{out}");
    }

    #[test]
    fn failed_optimize_then_gzip_keeps_original_content() {
        let (dir, plain) = setup(BROKEN);
        let cfg = config(true, Compression::Gzip);
        assert!(optimize_recording(&plain, &cfg).is_some());
        let result = finalize_recording(&plain, &cfg);
        assert_eq!(names(dir.path()), vec!["x.cast.gz"]);
        assert_eq!(read_gz(&result), BROKEN);
    }
}
