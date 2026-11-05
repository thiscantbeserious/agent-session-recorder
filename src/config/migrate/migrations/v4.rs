//! v3 -> v4: Add `storage.compression` (default "gzip").
//!
//! Nothing to rewrite: the missing key is filled from `Config::default()` after
//! migrations run, and an existing value is left alone.

use toml_edit::Table;

use crate::config::MigrateResult;

/// Target version this migration produces.
pub const VERSION: u32 = 4;

pub fn migrate(_root: &mut Table, _result: &mut MigrateResult) {}

#[cfg(test)]
mod tests {
    use crate::config::migrate::migrate_config;
    use crate::config::Compression;
    use crate::Config;

    #[test]
    fn v4_adds_gzip_compression() {
        let result = migrate_config("config_version = 3\n[storage]\n").unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.config_version, 5);
        assert_eq!(parsed.storage.compression, Compression::Gzip);
        assert!(result.content.contains("compression = \"gzip\""));
        assert!(result
            .added_fields
            .contains(&"storage.compression".to_string()));
    }

    #[test]
    fn v4_keeps_existing_none() {
        let input = "config_version = 3\n[storage]\ncompression = \"none\"\n";
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.storage.compression, Compression::None);
        assert!(!result
            .added_fields
            .contains(&"storage.compression".to_string()));
    }

    #[test]
    fn unknown_compression_names_allowed_values() {
        let err = toml::from_str::<Config>("[storage]\ncompression = \"zstd\"\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("gzip") && err.contains("none"), "{err}");
    }
}
