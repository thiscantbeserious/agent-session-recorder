//! v5 -> v6: Add the `[sync]` section (`auto_push = true`).
//!
//! Nothing to rewrite: `auto_push` is filled from `Config::default()` after
//! migrations run, and existing values are left alone.

use toml_edit::Table;

use crate::config::MigrateResult;

/// Target version this migration produces.
pub const VERSION: u32 = 6;

pub fn migrate(_root: &mut Table, _result: &mut MigrateResult) {}

#[cfg(test)]
mod tests {
    use crate::config::migrate::migrate_config;
    use crate::config::SyncBackendKind;
    use crate::Config;

    #[test]
    fn v6_adds_sync_section() {
        let result = migrate_config("config_version = 5\n").unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(
            parsed.config_version,
            crate::config::migrate::CURRENT_VERSION
        );
        assert!(parsed.sync.auto_push);
        assert_eq!(parsed.sync.backend, SyncBackendKind::Gist);
        assert!(result
            .content
            .contains("[sync]\nbackend = \"gist\"\nauto_push = true"));
    }

    #[test]
    fn v6_is_idempotent_and_keeps_values() {
        let input = "config_version = 5\n[sync]\nbackend = \"gist\"\nauto_push = false\n";
        let once = migrate_config(input).unwrap();
        let twice = migrate_config(&once.content).unwrap();
        assert_eq!(once.content, twice.content);
        assert!(!twice.has_changes());
        let parsed: Config = toml::from_str(&once.content).unwrap();
        assert_eq!(parsed.sync.backend, SyncBackendKind::Gist);
        assert!(!parsed.sync.auto_push);
    }

    #[test]
    fn unknown_backend_names_gist() {
        let result = toml::from_str::<Config>("[sync]\nbackend = \"s3\"\n");
        assert!(result.is_err(), "s3 must be rejected");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("gist"), "{err}");
    }
}
