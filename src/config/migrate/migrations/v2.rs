//! v1 -> v2: Update filename template to use {?branch} and {id}.
//!
//! Unconditionally overwrites `recording.filename_template` to the new default.

use toml_edit::{value, Item, Table};

use crate::config::MigrateResult;

/// Target version this migration produces.
pub const VERSION: u32 = 2;

/// New default filename template with optional branch and base36 id.
const NEW_TEMPLATE: &str = "{directory}{?branch}_{id}";

pub fn migrate(root: &mut Table, result: &mut MigrateResult) {
    overwrite_filename_template(root, result);
}

/// Unconditionally set `recording.filename_template` to the new default.
fn overwrite_filename_template(root: &mut Table, result: &mut MigrateResult) {
    let recording = root
        .entry("recording")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut();

    if let Some(recording) = recording {
        recording.insert("filename_template", value(NEW_TEMPLATE));
        result
            .removed_fields
            .push("recording.filename_template (overwritten)".to_string());
    }
}

#[cfg(test)]
mod tests {
    use crate::config::migrate::migrate_config;
    use crate::Config;

    #[test]
    fn v2_overwrites_old_default_template() {
        let input = r#"
config_version = 1

[recording]
filename_template = "{directory}_{date}_{time}"
"#;
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(
            parsed.recording.filename_template,
            "{directory}{?branch}_{id}"
        );
    }

    #[test]
    fn v2_overwrites_custom_template() {
        let input = r#"
config_version = 1

[recording]
filename_template = "{date}-custom"
"#;
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(
            parsed.recording.filename_template,
            "{directory}{?branch}_{id}"
        );
    }

    #[test]
    fn v2_config_is_idempotent() {
        let input = r#"
config_version = 2

[recording]
filename_template = "{directory}{?branch}_{id}"
"#;
        let result = migrate_config(input).unwrap();
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(
            parsed.recording.filename_template,
            "{directory}{?branch}_{id}"
        );
    }

    #[test]
    fn v0_config_migrates_through_v1_to_v2() {
        let input = r#"
[recording]
auto_analyze = false
analysis_agent = "claude"
filename_template = "{directory}_{date}_{time}"
"#;
        let result = migrate_config(input).unwrap();
        assert_eq!(result.old_version, 0);
        assert_eq!(result.new_version, 2);

        let parsed: Config = toml::from_str(&result.content).unwrap();
        // v1 migration moves analysis_agent -> analysis.agent
        assert_eq!(parsed.analysis.agent, Some("claude".to_string()));
        // v2 migration overwrites filename_template
        assert_eq!(
            parsed.recording.filename_template,
            "{directory}{?branch}_{id}"
        );
    }
}
