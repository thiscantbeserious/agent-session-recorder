//! Commented template lines for optional fields.

use super::{FieldDoc, AGENT_FIELDS, CONFIG_SECTIONS};
use std::collections::HashMap;

/// Insert commented-out template lines for optional fields that are absent.
///
/// Scans the TOML string for known sections and appends `# field = example`
/// lines for any documented fields not already present. This lets users
/// discover all available options even when their default is "unset".
pub fn insert_optional_field_templates(toml_str: &str) -> String {
    let mut lines: Vec<String> = toml_str.lines().map(str::to_string).collect();

    let present = collect_present_fields(&lines);

    let section_fields: Vec<(&str, &[FieldDoc])> = CONFIG_SECTIONS
        .iter()
        .map(|s| (s.name, s.fields))
        .chain(std::iter::once(("agents.claude", AGENT_FIELDS)))
        .chain(std::iter::once(("agents.codex", AGENT_FIELDS)))
        .chain(std::iter::once(("agents.gemini", AGENT_FIELDS)))
        .collect();

    // Process sections in reverse so line insertions don't shift indices
    for (section_name, fields) in section_fields.iter().rev() {
        let section_present = present.get(*section_name);
        let missing: Vec<&FieldDoc> = fields
            .iter()
            .filter(|f| {
                section_present
                    .map(|p| !p.iter().any(|k| k == f.name))
                    .unwrap_or(true)
            })
            .collect();

        if missing.is_empty() {
            continue;
        }

        insert_missing_for_section(&mut lines, section_name, &missing);
    }

    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Scan TOML lines and return the set of field names present in each section.
///
/// Both active fields (`key = value`) and commented-out templates (`# key = value`)
/// are counted as present so they are not duplicated on re-insertion.
fn collect_present_fields(lines: &[String]) -> HashMap<String, Vec<String>> {
    let mut present: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_section = String::new();

    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && !trimmed.starts_with("[[") {
            let name = trimmed
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim();
            current_section = name.to_string();
        } else if let Some((before_eq, _)) = trimmed.split_once('=') {
            let before_eq = before_eq.trim();
            let key = before_eq.strip_prefix('#').unwrap_or(before_eq).trim();
            if !key.is_empty() {
                present
                    .entry(current_section.clone())
                    .or_default()
                    .push(key.to_string());
            }
        }
    }

    present
}

/// Find the section's bounds in `lines` and insert template comments for `missing` fields.
///
/// Templates are placed right after the last non-blank content line inside the section,
/// before the next section header (or EOF). Has no effect if the section header is absent.
fn insert_missing_for_section(lines: &mut Vec<String>, section_name: &str, missing: &[&FieldDoc]) {
    let header = format!("[{}]", section_name);
    let Some(start) = lines.iter().position(|l| l.trim() == header) else {
        return;
    };

    let section_end = lines[start + 1..]
        .iter()
        .position(|l| {
            let t = l.trim();
            t.starts_with('[') && !t.starts_with("[[")
        })
        .map(|i| start + 1 + i)
        .unwrap_or(lines.len());

    // Find last non-blank content line so templates appear after the section's fields.
    let mut last_content = start;
    for (i, line) in lines.iter().enumerate().take(section_end).skip(start + 1) {
        if !line.trim().is_empty() {
            last_content = i;
        }
    }
    let insert_at = last_content + 1;

    let templates: Vec<String> = missing
        .iter()
        .map(|f| format!("# {} = {}", f.name, f.default_display))
        .collect();

    for (i, tmpl) in templates.into_iter().enumerate() {
        lines.insert(insert_at + i, tmpl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // collect_present_fields
    // -----------------------------------------------------------------------

    #[test]
    fn collect_present_fields_empty_input() {
        let lines: Vec<String> = vec![];
        let present = collect_present_fields(&lines);
        assert!(present.is_empty());
    }

    #[test]
    fn collect_present_fields_active_field() {
        let lines = vec!["[shell]".to_string(), "auto_wrap = true".to_string()];
        let present = collect_present_fields(&lines);
        let shell_fields = present
            .get("shell")
            .expect("shell section should be present");
        assert!(shell_fields.contains(&"auto_wrap".to_string()));
    }

    #[test]
    fn collect_present_fields_commented_template_counts_as_present() {
        // A commented-out template like `# agent = auto-detect` should be
        // treated as present so it is not duplicated on re-insertion.
        let lines = vec![
            "[analysis]".to_string(),
            "# agent = auto-detect".to_string(),
        ];
        let present = collect_present_fields(&lines);
        let fields = present.get("analysis").expect("analysis section missing");
        assert!(fields.contains(&"agent".to_string()));
    }

    #[test]
    fn collect_present_fields_array_of_tables_does_not_change_section() {
        // [[custom_array]] lines must not be treated as a section header.
        // Fields following them should remain in the last normal [section].
        let lines = vec![
            "[storage]".to_string(),
            "directory = \"~/r\"".to_string(),
            "[[custom_array]]".to_string(),
            "name = \"first\"".to_string(),
        ];
        let present = collect_present_fields(&lines);
        // `directory` belongs to `storage`
        let storage = present.get("storage").expect("storage section missing");
        assert!(storage.contains(&"directory".to_string()));
        // `name` should be associated with `storage` still (array of tables skipped)
        // It ends up in whatever section was current — the guard means [[...]] is ignored
        // What matters: no entry with key "custom_array" as a section
        assert!(!present.contains_key("custom_array"));
    }

    #[test]
    fn collect_present_fields_multi_section() {
        let lines = vec![
            "[shell]".to_string(),
            "auto_wrap = true".to_string(),
            "[storage]".to_string(),
            "directory = \"~/recordings\"".to_string(),
            "size_threshold_gb = 5.0".to_string(),
        ];
        let present = collect_present_fields(&lines);
        assert!(present
            .get("shell")
            .unwrap()
            .contains(&"auto_wrap".to_string()));
        let storage = present.get("storage").unwrap();
        assert!(storage.contains(&"directory".to_string()));
        assert!(storage.contains(&"size_threshold_gb".to_string()));
    }

    // -----------------------------------------------------------------------
    // insert_optional_field_templates — idempotency
    // -----------------------------------------------------------------------

    #[test]
    fn insert_optional_field_templates_idempotent() {
        // Running the function twice should produce the same output as once.
        let input = "[shell]\nauto_wrap = true\n\n[storage]\ndirectory = \"~/r\"\nsize_threshold_gb = 5.0\nage_threshold_days = 30\n";
        let once = insert_optional_field_templates(input);
        let twice = insert_optional_field_templates(&once);
        assert_eq!(
            once, twice,
            "insert_optional_field_templates must be idempotent"
        );
    }
}
