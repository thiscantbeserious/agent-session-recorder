//! Inline description comments for serialized configs.

use super::{AGENT_FIELDS, CONFIG_SECTIONS};
use std::collections::HashMap;

/// Annotate a serialized TOML config string with inline documentation comments.
///
/// Inserts `# description` comments above each known field.
pub fn annotate_config(toml_str: &str) -> String {
    // Build lookup: (section_name, field_name) -> description
    let mut lookup: HashMap<(&str, &str), &str> = HashMap::new();
    for section in CONFIG_SECTIONS {
        for field in section.fields {
            lookup.insert((section.name, field.name), field.description);
        }
    }
    // Per-agent fields apply to agents.claude, agents.codex, agents.gemini
    for field in AGENT_FIELDS {
        lookup.insert(("agents.claude", field.name), field.description);
        lookup.insert(("agents.codex", field.name), field.description);
        lookup.insert(("agents.gemini", field.name), field.description);
    }

    let mut result = String::new();
    let mut current_section = String::new();

    for line in toml_str.lines() {
        let trimmed = line.trim();

        // Track section headers
        if trimmed.starts_with('[') && !trimmed.starts_with("[[") {
            let name = trimmed
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim();
            current_section = name.to_string();
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // Check for field with a known description
        // Also match commented-out template lines like `# key = value`
        if let Some((before_eq, _)) = trimmed.split_once('=') {
            let raw_key = before_eq.trim();
            let key = raw_key.strip_prefix('#').unwrap_or(raw_key).trim();
            if let Some(desc) = lookup.get(&(current_section.as_str(), key)) {
                result.push_str(&format!("# {}\n", desc));
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotate_adds_comments() {
        let input = "[shell]\nauto_wrap = true\n";
        let output = annotate_config(input);
        assert!(output.contains("# Automatically wrap agent commands"));
        assert!(output.contains("auto_wrap = true"));
    }

    #[test]
    fn annotate_handles_agent_subsections() {
        let input = "[agents.claude]\nextra_args = []\n";
        let output = annotate_config(input);
        assert!(output.contains("# Default extra CLI arguments"));
    }

    #[test]
    fn annotate_preserves_unknown_fields() {
        let input = "[storage]\ndirectory = \"~/test\"\ncustom_field = 42\n";
        let output = annotate_config(input);
        assert!(output.contains("custom_field = 42"));
        // Unknown field should NOT have a comment
        assert!(!output.contains("# custom_field"));
    }
}
