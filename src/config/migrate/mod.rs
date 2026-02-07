//! Versioned config migration with format-preserving TOML edits.
//!
//! Each config file stores a `config_version` integer at the top level.
//! Missing version field means version 0 (pre-versioning legacy format).
//! Migrations run sequentially from stored version to `CURRENT_VERSION`,
//! then missing fields are filled from `Config::default()`.

mod migrations;

use anyhow::{Context, Result};
use toml_edit::{value, DocumentMut, Item, Table};

use super::Config;

/// The latest config schema version.
/// Bump this and add a migration file when the schema changes.
pub const CURRENT_VERSION: u32 = 1;

/// Result of a config migration operation
#[derive(Debug, Clone, Default)]
pub struct MigrateResult {
    /// The migrated config content (TOML string)
    pub content: String,
    /// True if the output differs from the input (e.g. reordering)
    pub content_changed: bool,
    /// Config version before migration
    pub old_version: u32,
    /// Config version after migration
    pub new_version: u32,
    /// List of fields that were added (format: "section.field")
    pub added_fields: Vec<String>,
    /// List of sections that were added
    pub sections_added: Vec<String>,
    /// List of deprecated fields that were removed/moved
    pub removed_fields: Vec<String>,
}

impl MigrateResult {
    /// Returns true if any changes were made (including reordering).
    pub fn has_changes(&self) -> bool {
        self.content_changed
            || !self.added_fields.is_empty()
            || !self.sections_added.is_empty()
            || !self.removed_fields.is_empty()
            || self.old_version != self.new_version
    }
}

/// Migrate an existing config to the latest schema version.
///
/// This function:
/// 1. Reads `config_version` from the file (defaults to 0 if absent)
/// 2. Runs versioned migrations sequentially (v0→v1, v1→v2, …)
/// 3. Fills missing fields from `Config::default()`
/// 4. Stamps `config_version = CURRENT_VERSION`
///
/// Preserves existing values, comments, formatting, and unknown fields.
pub fn migrate_config(existing_content: &str) -> Result<MigrateResult> {
    let mut result = MigrateResult::default();

    let mut doc: DocumentMut = existing_content
        .parse()
        .context("Failed to parse existing config as TOML")?;

    // Read stored version (absent = 0)
    let stored_version = doc
        .as_table()
        .get("config_version")
        .and_then(|v| v.as_integer())
        .map(|v| v as u32)
        .unwrap_or(0);

    result.old_version = stored_version;

    // Run versioned migrations from separate files
    migrations::run(stored_version, doc.as_table_mut(), &mut result);

    // Fill missing fields from defaults
    let default_config = Config::default();
    let default_toml =
        toml::to_string_pretty(&default_config).context("Failed to serialize default config")?;
    let default_doc: DocumentMut = default_toml
        .parse()
        .context("Failed to parse default config as TOML")?;

    fill_missing_defaults("", doc.as_table_mut(), default_doc.as_table(), &mut result);

    // Stamp current version
    doc.insert("config_version", value(i64::from(CURRENT_VERSION)));
    result.new_version = CURRENT_VERSION;

    // Validate that migrated values pass config validation (e.g. agent names).
    // This catches invalid values carried forward from old configs before writing.
    let migrated_config: Config =
        toml::from_str(&doc.to_string()).context("Migrated config is not valid TOML")?;
    migrated_config
        .analysis
        .validate()
        .map_err(|e| anyhow::anyhow!("Migrated config has invalid values: {}", e))?;

    // Sort sections first, then insert commented-out templates for optional
    // fields (Option<None> defaults). Templates must be inserted AFTER sorting
    // so they land in the correct (already-sorted) section positions. Running
    // insert_optional_field_templates on pure text avoids toml_edit re-parsing
    // which would reattach comment lines as prefix decorations on the wrong item.
    sort_sections(&mut doc);
    let content = super::docs::insert_optional_field_templates(&doc.to_string());

    result.content_changed = content != existing_content;
    result.content = content;
    Ok(result)
}

// ---------------------------------------------------------------------------
// Section ordering
// ---------------------------------------------------------------------------

/// Canonical order for top-level config sections.
/// Keys not in this list are appended at the end in their original order.
const SECTION_ORDER: &[&str] = &[
    "config_version",
    "shell",
    "storage",
    "recording",
    "analysis",
    "agents",
];

/// Reorder top-level sections to match `SECTION_ORDER`.
///
/// Works at the text level: splits the TOML into section blocks (grouping
/// sub-tables like `[agents.claude]` with their parent `agents`), then
/// reassembles them in canonical order. This preserves comments, unknown
/// fields, and all formatting within each section block.
fn sort_sections(doc: &mut DocumentMut) {
    let text = doc.to_string();
    let sorted = sort_toml_text(&text);
    if sorted != text {
        match sorted.parse::<DocumentMut>() {
            Ok(new_doc) => *doc = new_doc,
            Err(e) => eprintln!("Warning: could not reorder config sections: {}", e),
        }
    }
}

/// Split TOML text into section blocks and reassemble in canonical order.
fn sort_toml_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    // Find section header positions: (line_index, top_level_group_name)
    let mut headers: Vec<(usize, String)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // Match [section] and [section.sub] but not [[array_of_tables]]
        if let Some(name) = parse_section_header(trimmed) {
            let group = name.split('.').next().unwrap_or(&name).to_string();
            headers.push((i, group));
        }
    }

    if headers.is_empty() {
        return text.to_string();
    }

    // Preamble: everything before the first section header
    let preamble_end = headers[0].0;

    // Build section blocks: group consecutive headers that share a top-level group,
    // and also merge non-consecutive sections with the same group.
    // Each block is (group_name, text_content).
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut seen_groups: Vec<String> = Vec::new();

    for (idx, (start, group)) in headers.iter().enumerate() {
        let end = if idx + 1 < headers.len() {
            headers[idx + 1].0
        } else {
            lines.len()
        };

        let block_text = lines[*start..end].join("\n");

        // Append to existing block for this group, or create a new one
        if let Some(pos) = seen_groups.iter().position(|g| g == group) {
            blocks[pos].1.push('\n');
            blocks[pos].1.push_str(&block_text);
        } else {
            seen_groups.push(group.clone());
            blocks.push((group.clone(), block_text));
        }
    }

    // Sort blocks: known sections in SECTION_ORDER, then unknowns in original order
    let mut sorted_blocks: Vec<&(String, String)> = Vec::new();
    for &section in SECTION_ORDER {
        if let Some(block) = blocks.iter().find(|(g, _)| g == section) {
            sorted_blocks.push(block);
        }
    }
    for block in &blocks {
        if !SECTION_ORDER.contains(&block.0.as_str()) {
            sorted_blocks.push(block);
        }
    }

    // Reassemble
    let preamble = lines[..preamble_end].join("\n");
    let mut result = String::new();
    if !preamble.is_empty() {
        result.push_str(&preamble);
        if !result.ends_with('\n') {
            result.push('\n');
        }
    }
    for (i, block) in sorted_blocks.iter().enumerate() {
        if i > 0 || !preamble.is_empty() {
            // Ensure blank line between blocks (but don't double up)
            if !result.ends_with("\n\n") {
                if result.ends_with('\n') {
                    result.push('\n');
                } else {
                    result.push_str("\n\n");
                }
            }
        }
        result.push_str(&block.1);
        if !result.ends_with('\n') {
            result.push('\n');
        }
    }

    result
}

/// Extract section name from a TOML header line like `[section]` or `[section.sub]`.
/// Returns `None` for non-headers and `[[array_of_tables]]`.
fn parse_section_header(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with("[[") || !trimmed.starts_with('[') {
        return None;
    }
    let inner = trimmed.trim_start_matches('[');
    let name = inner.split(']').next()?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

// ---------------------------------------------------------------------------
// Default-fill (runs after all versioned migrations)
// ---------------------------------------------------------------------------

/// Recursively add missing fields/sections from the default config.
fn fill_missing_defaults(
    path: &str,
    user_table: &mut Table,
    default_table: &Table,
    result: &mut MigrateResult,
) {
    for (key, default_item) in default_table.iter() {
        // config_version is managed by the migration layer, not the default fill
        if key == "config_version" {
            continue;
        }

        let full_path = if path.is_empty() {
            key.to_string()
        } else {
            format!("{}.{}", path, key)
        };

        if !user_table.contains_key(key) {
            user_table.insert(key, default_item.clone());

            if path.is_empty() {
                result.sections_added.push(key.to_string());
            }

            track_added_fields(&full_path, default_item, result);
        } else {
            // Field exists — if both are tables, recurse
            if let (Some(u_table), Some(d_table)) = (
                user_table.get_mut(key).and_then(|i| i.as_table_mut()),
                default_item.as_table(),
            ) {
                fill_missing_defaults(&full_path, u_table, d_table, result);
            }
        }
    }
}

/// Recursively track all leaf fields added.
fn track_added_fields(path: &str, item: &Item, result: &mut MigrateResult) {
    if let Some(table) = item.as_table() {
        for (key, val) in table.iter() {
            track_added_fields(&format!("{}.{}", path, key), val, result);
        }
    } else {
        result.added_fields.push(path.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(result.removed_fields.is_empty());
    }

    // -----------------------------------------------------------------------
    // Default-fill
    // -----------------------------------------------------------------------

    #[test]
    fn empty_input_returns_full_default_config() {
        let result = migrate_config("").unwrap();

        assert_eq!(result.sections_added.len(), 5);
        assert!(result.sections_added.contains(&"storage".to_string()));
        assert!(result.sections_added.contains(&"agents".to_string()));
        assert!(result.sections_added.contains(&"shell".to_string()));
        assert!(result.sections_added.contains(&"recording".to_string()));
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

        assert_eq!(result.sections_added.len(), 4);
        assert!(result.sections_added.contains(&"agents".to_string()));
        assert!(result.sections_added.contains(&"shell".to_string()));
        assert!(result.sections_added.contains(&"recording".to_string()));
        assert!(result.sections_added.contains(&"analysis".to_string()));
        assert!(!result.sections_added.contains(&"storage".to_string()));

        assert!(result
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

        assert!(result
            .added_fields
            .contains(&"recording.filename_template".to_string()));
        assert!(result
            .added_fields
            .contains(&"recording.directory_max_length".to_string()));

        assert!(result.added_fields.contains(&"agents.no_wrap".to_string()));

        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.storage.directory, "~/custom");
        assert_eq!(parsed.storage.size_threshold_gb, 10.0);
        assert_eq!(parsed.agents.enabled, vec!["claude"]);
        assert!(!parsed.shell.auto_wrap);
        assert!(parsed.recording.auto_analyze);
    }

    #[test]
    fn complete_default_config_snapshot() {
        let result = migrate_config("").unwrap();
        insta::assert_snapshot!(result.content);

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
        assert_eq!(result.sections_added.len(), 5);
    }

    #[test]
    fn full_v0_to_v1_roundtrip() {
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
        assert_eq!(parsed.recording.filename_template, "{date}");

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
}
