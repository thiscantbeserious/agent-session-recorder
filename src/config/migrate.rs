//! Config migration: adds missing fields to user config while preserving formatting

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item};

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
/// - Walks the default config and adds any missing sections or fields
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

    // Walk default document and add missing sections/keys
    for (section_name, default_item) in default_doc.iter() {
        if let Item::Table(default_table) = default_item {
            if !doc.contains_key(section_name) {
                // Section doesn't exist - add entire section
                result.sections_added.push(section_name.to_string());
                doc[section_name] = default_item.clone();

                // Track all fields in this new section
                for (key, _) in default_table.iter() {
                    result
                        .added_fields
                        .push(format!("{}.{}", section_name, key));
                }
            } else if let Item::Table(user_table) = &doc[section_name] {
                // Section exists - check for missing keys
                let mut keys_to_add: Vec<(String, Item)> = Vec::new();

                for (key, default_value) in default_table.iter() {
                    if !user_table.contains_key(key) {
                        keys_to_add.push((key.to_string(), default_value.clone()));
                        result
                            .added_fields
                            .push(format!("{}.{}", section_name, key));
                    }
                }

                // Add missing keys to user's section
                if let Item::Table(user_table_mut) = &mut doc[section_name] {
                    for (key, value) in keys_to_add {
                        user_table_mut[&key] = value;
                    }
                }
            }
        }
    }

    result.content = doc.to_string();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_full_default_config() {
        let result = migrate_config("").unwrap();

        // Should have all 5 sections added
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
directory_max_length = 50

[analysis.agents]
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

[analysis.agents]
"#;

        let result = migrate_config(input).unwrap();

        // Should not add any fields (all present)
        assert!(!result.has_changes());

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

[analysis.agents]

[my.dotted.section]
value = 123
"#;

        let result = migrate_config(input).unwrap();

        // Should not add any fields (all required present)
        assert!(!result.has_changes());

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

[analysis.agents]
"#;

        let result = migrate_config(input).unwrap();

        // Should not add any fields (all required present)
        assert!(!result.has_changes());

        // Array of tables should be preserved
        assert!(result.content.contains("[[custom_array]]"));
        assert!(result.content.contains("name = \"first\""));
        assert!(result.content.contains("name = \"second\""));
    }
}
