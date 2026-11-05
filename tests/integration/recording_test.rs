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
