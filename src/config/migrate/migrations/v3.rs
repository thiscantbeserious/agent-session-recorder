//! v2 -> v3: Make the storage size warning opt-in.
//!
//! Removes `storage.size_threshold_gb` when it still holds the old default (5.0),
//! which earlier migrations wrote into every config. Custom values are kept.

use toml_edit::Table;

use crate::config::MigrateResult;

/// Target version this migration produces.
pub const VERSION: u32 = 3;

/// Former default that was filled into every config.
const OLD_DEFAULT_THRESHOLD_GB: f64 = 5.0;

pub fn migrate(root: &mut Table, result: &mut MigrateResult) {
    remove_default_size_threshold(root, result);
}

/// Drop `storage.size_threshold_gb` if it equals the old default.
fn remove_default_size_threshold(root: &mut Table, result: &mut MigrateResult) {
    let Some(storage) = root.get_mut("storage").and_then(|i| i.as_table_mut()) else {
        return;
    };
    let is_old_default = storage
        .get("size_threshold_gb")
        .and_then(|i| i.as_float().or_else(|| i.as_integer().map(|n| n as f64)))
        .is_some_and(|v| v == OLD_DEFAULT_THRESHOLD_GB);

    if is_old_default {
        storage.remove("size_threshold_gb");
        result
            .removed_fields
            .push("storage.size_threshold_gb".to_string());
    }
}

#[cfg(test)]
mod tests {
    use crate::config::migrate::migrate_config;
    use crate::Config;

    #[test]
    fn v3_removes_old_default_threshold() {
        let input = "config_version = 2\n\n[storage]\nsize_threshold_gb = 5.0\n";
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.storage.size_threshold_gb, None);
    }

    #[test]
    fn v3_removes_old_default_threshold_written_as_integer() {
        let input = "config_version = 2\n\n[storage]\nsize_threshold_gb = 5\n";
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.storage.size_threshold_gb, None);
    }

    #[test]
    fn v3_keeps_custom_threshold() {
        let input = "config_version = 2\n\n[storage]\nsize_threshold_gb = 20.0\n";
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.storage.size_threshold_gb, Some(20.0));
    }

    #[test]
    fn v3_does_not_touch_threshold_on_v3_config() {
        let input = "config_version = 3\n\n[storage]\nsize_threshold_gb = 5.0\n";
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.storage.size_threshold_gb, Some(5.0));
    }
}
