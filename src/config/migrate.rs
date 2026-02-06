//! Config migration: adds missing fields to user config while preserving formatting

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Table};

use super::Config;

/// Result of a config migration operation
#[derive(Debug, Clone, Default)]
pub struct MigrateResult {
    /// The migrated config content (TOML string)
    pub content: String,
    /// List of fields that were added (format: "section.field")
    pub added_fields: Vec<String>,
    /// List of sections that were added
    pub sections_added: Vec<String>,
}

impl MigrateResult {
    /// Returns true if any changes were made
    pub fn has_changes(&self) -> bool {
        !self.added_fields.is_empty() || !self.sections_added.is_empty()
    }
}

/// Migrate an existing config by adding missing fields from defaults.
///
/// This function:
/// - Parses the existing content with toml_edit (preserving formatting/comments)
/// - Generates the default config as a reference
/// - Walks the default config and adds any missing sections or fields recursively
/// - Preserves existing values, comments, formatting, and unknown fields
///
/// # Arguments
/// * `existing_content` - The current config file content (may be empty)
///
/// # Returns
/// * `MigrateResult` containing the new content and lists of what was added
pub fn migrate_config(existing_content: &str) -> Result<MigrateResult> {
    let mut result = MigrateResult::default();

    // Parse existing content (empty string parses to empty document)
    let mut doc: DocumentMut = existing_content
        .parse()
        .context("Failed to parse existing config as TOML")?;

    // Generate default config as reference
    let default_config = Config::default();
    let default_toml =
        toml::to_string_pretty(&default_config).context("Failed to serialize default config")?;
    let default_doc: DocumentMut = default_toml
        .parse()
        .context("Failed to parse default config as TOML")?;

    // Start recursive migration from the root
    migrate_recursive("", doc.as_table_mut(), default_doc.as_table(), &mut result);

    result.content = doc.to_string();
    Ok(result)
}

/// Recursively migrate fields from default_table to user_table.
fn migrate_recursive(
    path: &str,
    user_table: &mut Table,
    default_table: &Table,
    result: &mut MigrateResult,
) {
    for (key, default_item) in default_table.iter() {
        let full_path = if path.is_empty() {
            key.to_string()
        } else {
            format!("{}.{}", path, key)
        };

        if !user_table.contains_key(key) {
            // Field or section missing - add it entirely
            user_table.insert(key, default_item.clone());
            
            if path.is_empty() {
                result.sections_added.push(key.to_string());
            }
            
            // Track all leaf fields in the added item
            track_added_fields(&full_path, default_item, result);
        } else {
            // Field exists - if it's a table, recurse
            if let (Some(u_table), Some(d_table)) = (user_table.get_mut(key).and_then(|i| i.as_table_mut()), default_item.as_table()) {
                migrate_recursive(&full_path, u_table, d_table, result);
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
        // Leaf node (Value, Array, etc.)
        result.added_fields.push(path.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_full_default_config() {
        let result = migrate_config("").unwrap();

        // Should have all 5 top-level sections added
        assert_eq!(result.sections_added.len(), 5);
        assert!(result.sections_added.contains(&"storage".to_string()));
        assert!(result.sections_added.contains(&"agents".to_string()));
        assert!(result.sections_added.contains(&"shell".to_string()));
        assert!(result.sections_added.contains(&"recording".to_string()));
        assert!(result.sections_added.contains(&"analysis".to_string()));

        // Content should be valid TOML that parses back to Config
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

        // Should add 4 missing sections
        assert_eq!(result.sections_added.len(), 4);
        assert!(result.sections_added.contains(&"agents".to_string()));
        assert!(result.sections_added.contains(&"shell".to_string()));
        assert!(result.sections_added.contains(&"recording".to_string()));
        assert!(result.sections_added.contains(&"analysis".to_string()));
        assert!(!result.sections_added.contains(&"storage".to_string()));

        // Should add missing fields in storage section
        assert!(result
            .added_fields
            .contains(&"storage.size_threshold_gb".to_string()));
        assert!(result
            .added_fields
            .contains(&"storage.age_threshold_days".to_string()));

        // User's custom value should be preserved
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

        // Only [analysis] section should be added (others all exist)
        assert_eq!(result.sections_added.len(), 1);
        assert!(result.sections_added.contains(&"analysis".to_string()));

        // Should add missing fields in recording section
        assert!(result
            .added_fields
            .contains(&"recording.analysis_agent".to_string()));
        assert!(result
            .added_fields
            .contains(&"recording.filename_template".to_string()));
        assert!(result
            .added_fields
            .contains(&"recording.directory_max_length".to_string()));

        // Should add missing field in agents section
        assert!(result.added_fields.contains(&"agents.no_wrap".to_string()));

        // User values should be preserved
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.storage.directory, "~/custom");
        assert_eq!(parsed.storage.size_threshold_gb, 10.0);
        assert_eq!(parsed.agents.enabled, vec!["claude"]);
        assert!(!parsed.shell.auto_wrap);
        assert!(parsed.recording.auto_analyze);
    }

    #[test]
    fn complete_config_returns_no_changes() {
        let input = r#"
[storage]
directory = "~/recorded_agent_sessions"
size_threshold_gb = 5.0
age_threshold_days = 30

[agents]
enabled = ["claude", "codex", "gemini"]
no_wrap = []

[shell]
auto_wrap = true

[recording]
auto_analyze = false
analysis_agent = "claude"
filename_template = "{directory}_{date}_{time}"
directory_max_length = 15

[analysis]
default_agent = "claude"
timeout = 120
fast = false
curate = true

[analysis.agents.claude]
extra_args = []

[analysis.agents.codex]
extra_args = []

[analysis.agents.gemini]
extra_args = []
"#;

        let result = migrate_config(input).unwrap();

        assert!(result.sections_added.is_empty());
        assert!(result.added_fields.is_empty());
        assert!(!result.has_changes());
    }

    #[test]
    fn comments_are_preserved() {
        let input = r#"
# My custom config
[storage]
# Store recordings here
directory = "~/my-recordings"
"#;

        let result = migrate_config(input).unwrap();

        // Comments should be preserved in output
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

        // Unknown fields should be preserved
        assert!(result.content.contains("custom_unknown_field"));
        assert!(result.content.contains("should stay"));
        assert!(result.content.contains("[my_custom_section]"));
        assert!(result.content.contains("foo"));
    }

    #[test]
    fn has_changes_returns_correctly() {
        // Empty result
        let empty = MigrateResult::default();
        assert!(!empty.has_changes());

        // With sections added
        let with_sections = MigrateResult {
            content: String::new(),
            added_fields: vec![],
            sections_added: vec!["storage".to_string()],
        };
        assert!(with_sections.has_changes());

        // With fields added
        let with_fields = MigrateResult {
            content: String::new(),
            added_fields: vec!["storage.directory".to_string()],
            sections_added: vec![],
        };
        assert!(with_fields.has_changes());
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

        // Should add all sections like empty input
        assert_eq!(result.sections_added.len(), 5);
    }

    #[test]
    fn inline_tables_in_user_config_handled() {
        // User might have inline table format
        let input = r#"
[storage]
directory = "~/recordings"
size_threshold_gb = 5.0
age_threshold_days = 30

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

        // Custom values preserved
        let parsed: Config = toml::from_str(&result.content).unwrap();
        assert_eq!(parsed.recording.filename_template, "{date}");
        assert_eq!(parsed.recording.directory_max_length, 25);
    }

    #[test]
    fn nested_dotted_tables_are_preserved() {
        // User might have custom nested tables or dotted section names
        // Migration should preserve them without modification
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

        // Nested and dotted sections should be preserved
        assert!(result.content.contains("[storage.custom]"));
        assert!(result.content.contains("nested_field"));
        assert!(result.content.contains("[my.dotted.section]"));
        assert!(result.content.contains("value = 123"));
    }

    #[test]
    fn array_of_tables_preserved() {
        // TOML array of tables syntax should be preserved
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

        // Array of tables should be preserved
        assert!(result.content.contains("[[custom_array]]"));
        assert!(result.content.contains("name = \"first\""));
        assert!(result.content.contains("name = \"second\""));
    }
}