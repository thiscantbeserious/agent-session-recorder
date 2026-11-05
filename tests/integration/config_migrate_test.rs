//! Tests for `migrate_config`: version handling, default-fill and preservation.

use agr::config::{migrate_config, CURRENT_VERSION};
use agr::Config;

// -----------------------------------------------------------------------
// Version reading & stamping
// -----------------------------------------------------------------------

#[test]
fn no_version_field_means_version_zero() {
    let input = r#"
[storage]
directory = "~/recordings"
"#;
    let result = migrate_config(input).unwrap();
    assert_eq!(result.old_version, 0);
    assert_eq!(result.new_version, CURRENT_VERSION);
}

#[test]
fn explicit_version_is_read() {
    let input = format!(
        "config_version = {}\n\n[storage]\ndirectory = \"~/recordings\"\n",
        CURRENT_VERSION
    );
    let result = migrate_config(&input).unwrap();
    assert_eq!(result.old_version, CURRENT_VERSION);
    assert_eq!(result.new_version, CURRENT_VERSION);
}

#[test]
fn version_is_stamped_in_output() {
    let result = migrate_config("").unwrap();
    assert!(result
        .content
        .contains(&format!("config_version = {}", CURRENT_VERSION)));
}

#[test]
fn v1_config_skips_v0_migration() {
    let input = r#"
config_version = 1

[recording]
auto_analyze = false
filename_template = "{directory}_{date}_{time}"
directory_max_length = 15

[analysis]
agent = "claude"
"#;
    let result = migrate_config(input).unwrap();
    // v2 migration overwrites filename_template, so removed_fields contains that entry
    assert!(!result
        .removed_fields
        .iter()
        .any(|f| f == "recording.analysis_agent"));
}

// -----------------------------------------------------------------------
// Default-fill
// -----------------------------------------------------------------------

#[test]
fn empty_input_returns_full_default_config() {
    let result = migrate_config("").unwrap();

    // v2 migration creates [recording] with filename_template,
    // so recording is no longer in sections_added (only 4 sections added by default-fill)
    assert_eq!(result.sections_added.len(), 4);
    assert!(result.sections_added.contains(&"storage".to_string()));
    assert!(result.sections_added.contains(&"agents".to_string()));
    assert!(result.sections_added.contains(&"shell".to_string()));
    assert!(result.sections_added.contains(&"analysis".to_string()));

    let parsed: Config = toml::from_str(&result.content).unwrap();
    assert_eq!(parsed.storage.directory, "~/recorded_agent_sessions");
    assert_eq!(parsed.agents.enabled.len(), 3);
    assert!(parsed.shell.auto_wrap);
}

#[test]
fn config_with_one_section_adds_other_sections() {
    let input = r#"
[storage]
directory = "~/my-recordings"
"#;

    let result = migrate_config(input).unwrap();

    // v2 migration creates [recording] with filename_template, so only 3 sections added
    assert_eq!(result.sections_added.len(), 3);
    assert!(result.sections_added.contains(&"agents".to_string()));
    assert!(result.sections_added.contains(&"shell".to_string()));
    assert!(result.sections_added.contains(&"analysis".to_string()));
    assert!(!result.sections_added.contains(&"storage".to_string()));

    assert!(!result
        .added_fields
        .contains(&"storage.size_threshold_gb".to_string()));
    assert!(result
        .added_fields
        .contains(&"storage.age_threshold_days".to_string()));

    let parsed: Config = toml::from_str(&result.content).unwrap();
    assert_eq!(parsed.storage.directory, "~/my-recordings");
}

#[test]
fn partial_section_adds_missing_fields() {
    let input = r#"
[storage]
directory = "~/custom"
size_threshold_gb = 10.0
age_threshold_days = 60

[agents]
enabled = ["claude"]

[shell]
auto_wrap = false

[recording]
auto_analyze = true
"#;

    let result = migrate_config(input).unwrap();

    assert_eq!(result.sections_added.len(), 1);
    assert!(result.sections_added.contains(&"analysis".to_string()));

    // v2 migration sets filename_template, so it's not in added_fields
    assert!(result
        .added_fields
        .contains(&"recording.directory_max_length".to_string()));

    assert!(result.added_fields.contains(&"agents.no_wrap".to_string()));

    let parsed: Config = toml::from_str(&result.content).unwrap();
    assert_eq!(parsed.storage.directory, "~/custom");
    assert_eq!(parsed.storage.size_threshold_gb, Some(10.0));
    assert_eq!(parsed.agents.enabled, vec!["claude"]);
    assert!(!parsed.shell.auto_wrap);
    assert!(parsed.recording.auto_analyze);
    assert_eq!(
        parsed.recording.filename_template,
        "{directory}{?branch}_{id}"
    );
}

#[test]
fn complete_default_config_snapshot() {
    let result = migrate_config("").unwrap();
    insta::with_settings!({prepend_module_to_snapshot => false, snapshot_path => "snapshots"}, {
        insta::assert_snapshot!(
            "agr__config__migrate__tests__complete_default_config_snapshot",
            result.content
        );
    });

    // Re-migrating the output should be a no-op
    let result2 = migrate_config(&result.content).unwrap();
    assert!(!result2.has_changes(), "re-migration should be a no-op");
}

// -----------------------------------------------------------------------
// Preservation
// -----------------------------------------------------------------------

#[test]
fn comments_are_preserved() {
    let input = r#"
# My custom config
[storage]
# Store recordings here
directory = "~/my-recordings"
"#;

    let result = migrate_config(input).unwrap();

    assert!(result.content.contains("# My custom config"));
    assert!(result.content.contains("# Store recordings here"));
}

#[test]
fn unknown_fields_are_preserved() {
    let input = r#"
[storage]
directory = "~/recordings"
size_threshold_gb = 5.0
age_threshold_days = 30
custom_unknown_field = "should stay"

[my_custom_section]
foo = "bar"
"#;

    let result = migrate_config(input).unwrap();

    assert!(result.content.contains("custom_unknown_field"));
    assert!(result.content.contains("should stay"));
    assert!(result.content.contains("[my_custom_section]"));
    assert!(result.content.contains("foo"));
}

#[test]
fn nested_dotted_tables_are_preserved() {
    let input = r#"
[storage]
directory = "~/recordings"
size_threshold_gb = 5.0
age_threshold_days = 30

[storage.custom]
nested_field = "preserved"

[agents]
enabled = ["claude"]
no_wrap = []

[shell]
auto_wrap = true

[recording]
auto_analyze = false
analysis_agent = "claude"
filename_template = "{date}"
directory_max_length = 25

[analysis]
default_agent = "claude"

[my.dotted.section]
value = 123
"#;

    let result = migrate_config(input).unwrap();

    assert!(result.content.contains("[storage.custom]"));
    assert!(result.content.contains("nested_field"));
    assert!(result.content.contains("[my.dotted.section]"));
    assert!(result.content.contains("value = 123"));
}

#[test]
fn array_of_tables_preserved() {
    let input = r#"
[storage]
directory = "~/recordings"
size_threshold_gb = 5.0
age_threshold_days = 30

[[custom_array]]
name = "first"

[[custom_array]]
name = "second"

[agents]
enabled = ["claude"]
no_wrap = []

[shell]
auto_wrap = true

[recording]
auto_analyze = false
analysis_agent = "claude"
filename_template = "{date}"
directory_max_length = 25

[analysis]
default_agent = "claude"
"#;

    let result = migrate_config(input).unwrap();

    assert!(result.content.contains("[[custom_array]]"));
    assert!(result.content.contains("name = \"first\""));
    assert!(result.content.contains("name = \"second\""));
}
