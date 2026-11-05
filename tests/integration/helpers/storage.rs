//! Shared config and session builders for storage tests

use super::setup_test_sessions;
use agr::{Config, StorageManager};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub fn create_test_config(temp_dir: &TempDir) -> Config {
    let mut config = Config::default();
    config.storage.directory = temp_dir.path().to_string_lossy().to_string();
    config
}

pub fn create_test_session(dir: &Path, agent: &str, filename: &str, content: &str) -> PathBuf {
    let agent_dir = dir.join(agent);
    fs::create_dir_all(&agent_dir).unwrap();
    let path = agent_dir.join(filename);
    fs::write(&path, content).unwrap();
    path
}

/// Create a test config with a custom storage directory.
pub fn create_dir_config(storage_dir: &Path) -> Config {
    Config {
        storage: agr::config::StorageConfig {
            directory: storage_dir.to_string_lossy().to_string(),
            size_threshold_gb: Some(5.0),
            age_threshold_days: 30,
            ..Default::default()
        },
        agents: agr::config::AgentsConfig {
            enabled: vec!["claude".to_string(), "codex".to_string()],
            no_wrap: vec![],
            ..Default::default()
        },
        shell: Default::default(),
        recording: Default::default(),
        analysis: Default::default(),
        ..Default::default()
    }
}

pub fn create_test_manager() -> (TempDir, StorageManager) {
    let temp_dir = TempDir::new().unwrap();
    let sessions_dir = setup_test_sessions(&temp_dir);

    let mut config = Config::default();
    config.storage.directory = sessions_dir.to_string_lossy().to_string();

    let manager = StorageManager::new(config);
    (temp_dir, manager)
}

/// Write a session through the codec, gzip or plain by the file suffix.
pub fn create_codec_session(dir: &Path, agent: &str, filename: &str, content: &str) -> PathBuf {
    let agent_dir = dir.join(agent);
    fs::create_dir_all(&agent_dir).unwrap();
    let path = agent_dir.join(filename);
    agr::files::codec::atomic_write(&path, |w| Ok(w.write_all(content.as_bytes())?)).unwrap();
    path
}

/// Config with a storage directory and the given compression.
pub fn compression_config(temp_dir: &TempDir, compression: agr::config::Compression) -> Config {
    let mut config = create_test_config(temp_dir);
    config.storage.compression = compression;
    config
}
