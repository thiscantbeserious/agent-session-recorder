//! v4 -> v5: Add `recording.auto_optimize` (default true).
//!
//! Nothing to rewrite: the missing key is filled from `Config::default()` after
//! migrations run, and an existing value is left alone.

use toml_edit::Table;

use crate::config::MigrateResult;

/// Target version this migration produces.
pub const VERSION: u32 = 5;

pub fn migrate(_root: &mut Table, _result: &mut MigrateResult) {}

#[cfg(test)]
mod tests {
    use crate::config::migrate::migrate_config;
    use crate::Config;

    const KEY: &str = "recording.auto_optimize";

    #[test]
    fn v5_adds_auto_optimize_true() {
        let result = migrate_config("config_version = 4\n[recording]\n").unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(
            parsed.config_version,
            crate::config::migrate::CURRENT_VERSION
        );
        assert!(parsed.recording.auto_optimize);
        assert!(result.content.contains("auto_optimize = true"));
        assert!(result.added_fields.contains(&KEY.to_string()));
    }

    #[test]
    fn v5_keeps_existing_false() {
        let input = "config_version = 4\n[recording]\nauto_optimize = false\n";
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert!(!parsed.recording.auto_optimize);
        assert!(!result.added_fields.contains(&KEY.to_string()));
    }

    #[test]
    fn v5_is_idempotent() {
        let first = migrate_config("config_version = 4\n[recording]\n").unwrap();
        let second = migrate_config(&first.content).unwrap();
        assert!(!second.has_changes());
    }

    #[test]
    fn missing_key_defaults_to_true() {
        let parsed: Config = toml::from_str("[recording]\n").unwrap();
        assert!(parsed.recording.auto_optimize);
    }

    #[test]
    fn non_bool_names_field() {
        let result = toml::from_str::<Config>("[recording]\nauto_optimize = \"yes\"\n");
        assert!(result.is_err());
        let err = result.map(|_| ()).unwrap_err().to_string();
        assert!(err.contains("auto_optimize"), "{err}");
    }
}
