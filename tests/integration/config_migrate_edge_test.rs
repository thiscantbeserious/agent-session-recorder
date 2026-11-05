//! Tests for `migrate_config`: edge cases, full roundtrip and section reordering.

use agr::config::{migrate_config, MigrateResult, CURRENT_VERSION};
use agr::Config;

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[test]
fn has_changes_returns_correctly() {
    let empty = MigrateResult::default();
    assert!(!empty.has_changes());

    let with_sections = MigrateResult {
        sections_added: vec!["storage".to_string()],
        ..Default::default()
    };
    assert!(with_sections.has_changes());

    let with_fields = MigrateResult {
        added_fields: vec!["storage.directory".to_string()],
        ..Default::default()
    };
    assert!(with_fields.has_changes());

    let with_removed = MigrateResult {
        removed_fields: vec!["recording.analysis_agent".to_string()],
        ..Default::default()
    };
    assert!(with_removed.has_changes());

    let with_version_bump = MigrateResult {
        old_version: 0,
        new_version: 1,
        ..Default::default()
    };
    assert!(with_version_bump.has_changes());
}

#[test]
fn invalid_toml_returns_error() {
    let input = "this is not valid { toml }}}";
    let result = migrate_config(input);
    assert!(result.is_err());
}

#[test]
fn whitespace_only_input_treated_as_empty() {
    let result = migrate_config("   \n\n   ").unwrap();
    // v2 migration creates [recording], so only 4 sections added
    assert_eq!(result.sections_added.len(), 4);
}

#[test]
fn full_v0_to_latest_roundtrip() {
    let input = r#"
[storage]
directory = "~/recordings"
size_threshold_gb = 5.0
age_threshold_days = 30

[agents]
enabled = ["claude", "codex"]
no_wrap = []

[shell]
auto_wrap = true

[recording]
auto_analyze = true
analysis_agent = "codex"
filename_template = "{date}"
directory_max_length = 20

[analysis]
default_agent = "codex"
timeout = 180

[analysis.agents.codex]
extra_args = ["--model", "gpt-5.2-codex"]
"#;

    let result = migrate_config(input).unwrap();
    assert_eq!(result.old_version, 0);
    assert_eq!(result.new_version, CURRENT_VERSION);

    assert!(result
        .removed_fields
        .contains(&"recording.analysis_agent".to_string()));
    assert!(result
        .removed_fields
        .contains(&"analysis.default_agent".to_string()));
    assert!(result
        .removed_fields
        .contains(&"analysis.agents.codex".to_string()));

    let parsed: Config = toml::from_str(&result.content).unwrap();
    assert_eq!(parsed.analysis.agent, Some("codex".to_string()));
    assert_eq!(
        parsed.agents.codex.extra_args,
        vec!["--model", "gpt-5.2-codex"]
    );
    assert_eq!(parsed.analysis.timeout, Some(180));
    assert!(parsed.recording.auto_analyze);
    // v2 migration unconditionally overwrites filename_template
    assert_eq!(
        parsed.recording.filename_template,
        "{directory}{?branch}_{id}"
    );

    assert!(result
        .content
        .contains(&format!("config_version = {}", CURRENT_VERSION)));

    // Re-migrating is a no-op
    let result2 = migrate_config(&result.content).unwrap();
    assert_eq!(result2.old_version, CURRENT_VERSION);
    assert!(result2.removed_fields.is_empty());
}

#[test]
fn misordered_sections_detected_as_change() {
    let input = r#"config_version = 1

[storage]
directory = "~/recordings"
size_threshold_gb = 5.0
age_threshold_days = 30

[agents]
enabled = ["claude"]
no_wrap = []

[agents.claude]
extra_args = []
analyze_extra_args = []
curate_extra_args = []
rename_extra_args = []

[shell]
auto_wrap = true

[agents.codex]
extra_args = []
analyze_extra_args = []
curate_extra_args = []
rename_extra_args = []

[recording]
auto_analyze = false
filename_template = "{date}"
directory_max_length = 15

[agents.gemini]
extra_args = []
analyze_extra_args = []
curate_extra_args = []
rename_extra_args = []

[analysis]
timeout = 120
fast = false
curate = true
"#;
    let result = migrate_config(input).unwrap();
    assert!(result.content_changed, "reordering should be detected");
    assert!(result.has_changes());

    // Verify correct order in output
    let shell_pos = result.content.find("[shell]").unwrap();
    let storage_pos = result.content.find("[storage]").unwrap();
    let recording_pos = result.content.find("[recording]").unwrap();
    let analysis_pos = result.content.find("[analysis]").unwrap();
    let agents_pos = result.content.find("[agents]").unwrap();
    assert!(shell_pos < storage_pos);
    assert!(storage_pos < recording_pos);
    assert!(recording_pos < analysis_pos);
    assert!(analysis_pos < agents_pos);
}
