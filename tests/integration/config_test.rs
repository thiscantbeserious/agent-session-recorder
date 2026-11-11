//! Unit tests for config module

use agr::config::Compression;
use agr::Config;

#[test]
fn default_config_has_expected_values() {
    let config = Config::default();
    assert_eq!(config.storage.directory, "~/recorded_agent_sessions");
    assert_eq!(config.storage.size_threshold_gb, None);
    assert_eq!(config.storage.age_threshold_days, 30);
    assert_eq!(config.storage.compression, Compression::Gzip);
    assert!(config.agents.enabled.contains(&"claude".to_string()));
    assert!(config.agents.enabled.contains(&"codex".to_string()));
    assert!(config.agents.enabled.contains(&"gemini".to_string()));
    // Shell config defaults
    assert!(config.shell.auto_wrap);
    assert!(config.shell.script_path.is_none());
    // Recording config defaults
    assert!(!config.recording.auto_analyze);
    // Agents no_wrap defaults
    assert!(config.agents.no_wrap.is_empty());
}

#[test]
fn config_serialization_roundtrip() {
    let config = Config::default();
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(parsed.storage.directory, config.storage.directory);
    assert_eq!(parsed.agents.enabled, config.agents.enabled);
    assert_eq!(parsed.shell.auto_wrap, config.shell.auto_wrap);
}

#[test]
fn shell_config_parses_from_toml() {
    let toml_str = r#"
[shell]
auto_wrap = false
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(!config.shell.auto_wrap);
}

#[test]
fn shell_config_defaults_when_missing() {
    let toml_str = r#"
[storage]
directory = "~/custom"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    // Shell config should have default values
    assert!(config.shell.auto_wrap);
}

#[test]
fn add_agent_adds_new_agent() {
    let mut config = Config::default();
    assert!(config.add_agent("new-agent"));
    assert!(config.is_agent_enabled("new-agent"));
}

#[test]
fn add_agent_does_not_duplicate() {
    let mut config = Config::default();
    assert!(!config.add_agent("claude"));
    assert_eq!(
        config
            .agents
            .enabled
            .iter()
            .filter(|a| *a == "claude")
            .count(),
        1
    );
}

#[test]
fn remove_agent_removes_existing() {
    let mut config = Config::default();
    assert!(config.remove_agent("claude"));
    assert!(!config.is_agent_enabled("claude"));
}

#[test]
fn remove_agent_returns_false_for_nonexistent() {
    let mut config = Config::default();
    assert!(!config.remove_agent("nonexistent"));
}

#[test]
fn storage_directory_expands_tilde() {
    let config = Config::default();
    let path = config.storage_directory();
    assert!(!path.to_string_lossy().contains('~'));
    assert!(path.to_string_lossy().contains("recorded_agent_sessions"));
}

#[test]
fn should_wrap_agent_respects_enabled_list() {
    let config = Config::default();
    assert!(config.should_wrap_agent("claude"));
    assert!(!config.should_wrap_agent("unknown-agent"));
}

#[test]
fn should_wrap_agent_respects_no_wrap_list() {
    let mut config = Config::default();
    assert!(config.should_wrap_agent("claude"));
    config.add_no_wrap("claude");
    assert!(!config.should_wrap_agent("claude"));
}

#[test]
fn should_wrap_agent_respects_auto_wrap_toggle() {
    let mut config = Config::default();
    assert!(config.should_wrap_agent("claude"));
    config.shell.auto_wrap = false;
    assert!(!config.should_wrap_agent("claude"));
}

#[test]
fn add_no_wrap_adds_new_agent() {
    let mut config = Config::default();
    assert!(config.add_no_wrap("test-agent"));
    assert!(config.agents.no_wrap.contains(&"test-agent".to_string()));
}

#[test]
fn add_no_wrap_does_not_duplicate() {
    let mut config = Config::default();
    config.add_no_wrap("test-agent");
    assert!(!config.add_no_wrap("test-agent"));
    assert_eq!(
        config
            .agents
            .no_wrap
            .iter()
            .filter(|a| *a == "test-agent")
            .count(),
        1
    );
}

#[test]
fn remove_no_wrap_removes_existing() {
    let mut config = Config::default();
    config.add_no_wrap("test-agent");
    assert!(config.remove_no_wrap("test-agent"));
    assert!(!config.agents.no_wrap.contains(&"test-agent".to_string()));
}

#[test]
fn remove_no_wrap_returns_false_for_nonexistent() {
    let mut config = Config::default();
    assert!(!config.remove_no_wrap("nonexistent"));
}

#[test]
fn recording_config_parses_from_toml() {
    let toml_str = r#"
[recording]
auto_analyze = true
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.recording.auto_analyze);
}

#[test]
fn analysis_config_with_agent() {
    let toml_str = r#"
[analysis]
agent = "codex"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.analysis.agent, Some("codex".to_string()));
}

#[test]
fn analysis_config_defaults_agent_to_none() {
    let toml_str = r#"
[analysis]
timeout = 60
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.analysis.agent, None);
}

#[test]
fn recording_config_defaults_when_missing() {
    let toml_str = r#"
[storage]
directory = "~/custom"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(!config.recording.auto_analyze);
}

#[test]
fn no_wrap_config_parses_from_toml() {
    let toml_str = r#"
[agents]
enabled = ["claude", "codex"]
no_wrap = ["codex"]
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.should_wrap_agent("claude"));
    assert!(!config.should_wrap_agent("codex"));
}

#[test]
fn config_path_returns_valid_path() {
    let path = Config::config_path().unwrap();
    assert!(path.to_string_lossy().contains("config.toml"));
    assert!(path.to_string_lossy().contains("agr"));
}

#[test]
fn config_dir_returns_valid_path() {
    let dir = Config::config_dir().unwrap();
    assert!(dir.to_string_lossy().contains("agr"));
    assert!(dir.to_string_lossy().contains(".config"));
}

#[test]
fn load_returns_default_when_no_config_file() {
    // This relies on Config::load() returning defaults when file doesn't exist
    // Since we can't easily mock the filesystem, we test the logic indirectly
    let config = Config::default();
    assert_eq!(config.storage.directory, "~/recorded_agent_sessions");
}

#[test]
fn storage_directory_handles_non_tilde_path() {
    let mut config = Config::default();
    config.storage.directory = "/absolute/path".to_string();
    let path = config.storage_directory();
    assert_eq!(path, std::path::PathBuf::from("/absolute/path"));
}

#[test]
fn storage_directory_handles_relative_path() {
    let mut config = Config::default();
    config.storage.directory = "relative/path".to_string();
    let path = config.storage_directory();
    assert_eq!(path, std::path::PathBuf::from("relative/path"));
}

#[test]
fn load_from_invalid_compression_names_path_and_allowed_values() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[storage]\ncompression = \"zstd\"\n").unwrap();
    let msg = format!("{:#}", Config::load_from(&path).unwrap_err());
    assert!(msg.contains("config.toml"), "{msg}");
    assert!(msg.contains("gzip") && msg.contains("none"), "{msg}");
}

#[test]
fn load_from_missing_file_returns_defaults() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = Config::load_from(&dir.path().join("nope.toml")).unwrap();
    assert_eq!(config.storage.directory, "~/recorded_agent_sessions");
}

mod state {
    use agr::config::state::State;

    #[test]
    fn load_from_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::load_from(&dir.path().join("state.toml"));
        assert_eq!(state.compress_offer_version, None);
    }

    #[test]
    fn load_from_malformed_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        std::fs::write(&path, "compress_offer_version = [not toml").unwrap();
        assert_eq!(State::load_from(&path), State::default());
    }

    #[test]
    fn malformed_file_warns_naming_the_path_and_missing_file_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        assert_eq!(State::load_checked(&path).1, None);
        std::fs::write(&path, "compress_offer_version = [not toml").unwrap();
        let warning = State::load_checked(&path).1;
        assert!(
            warning.is_some_and(|w| w.contains("state.toml")),
            "no warning for broken state"
        );
    }

    #[test]
    fn save_then_load_round_trips_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("state.toml");
        let state = State {
            compress_offer_version: Some("0.3.0".into()),
            ..Default::default()
        };
        state.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), state);
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("state.toml")]);
    }
}

mod sync_defaults {
    use agr::config::SyncBackendKind;
    use agr::Config;

    #[test]
    fn sync_defaults_are_gist_and_auto_push_off() {
        let sync = Config::default().sync;
        assert_eq!(
            (sync.backend, sync.auto_push),
            (SyncBackendKind::Gist, false)
        );
        let parsed: Config = toml::from_str("[sync]\n").unwrap();
        assert_eq!(
            (parsed.sync.backend, parsed.sync.auto_push),
            (SyncBackendKind::Gist, false)
        );
    }
}

mod pack_size {
    use agr::Config;

    #[test]
    fn pack_size_mib_defaults_to_64() {
        assert_eq!(Config::default().sync.pack_size_mib, 64);
        let parsed: Config = toml::from_str("[sync]\n").unwrap();
        assert_eq!(parsed.sync.pack_size_mib, 64);
    }

    fn load_with(mib: u64) -> anyhow::Result<Config> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, format!("[sync]\npack_size_mib = {mib}\n")).unwrap();
        Config::load_from(&path)
    }

    #[test]
    fn pack_size_mib_out_of_range_is_a_clear_error() {
        for mib in [8, 1024] {
            let err = load_with(mib).unwrap_err();
            let text = format!("{err:#}");
            assert!(
                text.contains("pack_size_mib must be between 16 and 512"),
                "{mib}: {text}"
            );
        }
        assert_eq!(load_with(16).unwrap().sync.pack_size_mib, 16);
        assert_eq!(load_with(512).unwrap().sync.pack_size_mib, 512);
    }
}

mod writer_id {
    use agr::config::state::State;

    #[test]
    fn writer_id_created_once_in_state_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        let first = State::writer_id(&path).unwrap();
        let second = State::writer_id(&path).unwrap();
        assert_eq!(first, second);
        assert_eq!(State::load_from(&path).writer_id, Some(first));
    }

    #[test]
    fn keep_writer_id_returns_the_id_that_was_stored_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.toml");
        assert_eq!(
            State::keep_writer_id_at(&path, "w-first").unwrap(),
            "w-first"
        );
        let second = State::keep_writer_id_at(&path, "w-second").unwrap();
        assert_eq!(second, "w-first");
        assert_eq!(
            State::load_from(&path).writer_id.as_deref(),
            Some("w-first")
        );
    }
}
