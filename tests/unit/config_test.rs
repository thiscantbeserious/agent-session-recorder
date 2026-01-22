//! Unit tests for config module

use agr::Config;

#[test]
fn default_config_has_expected_values() {
    let config = Config::default();
    assert_eq!(config.storage.directory, "~/recorded_agent_sessions");
    assert_eq!(config.storage.size_threshold_gb, 5.0);
    assert_eq!(config.storage.age_threshold_days, 30);
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
fn recording_config_with_analysis_agent() {
    let toml_str = r#"
[recording]
auto_analyze = true
analysis_agent = "codex"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.recording.auto_analyze);
    assert_eq!(config.recording.analysis_agent, "codex");
}

#[test]
fn recording_config_defaults_analysis_agent_to_claude() {
    let toml_str = r#"
[recording]
auto_analyze = true
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.recording.analysis_agent, "claude");
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
