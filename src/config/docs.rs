//! Config field documentation — single source of truth for descriptions.
//!
//! Used by:
//! - `agr config show` to annotate TOML output with inline comments
//! - `cargo xtask gen-docs` to generate `wiki/Configuration.md`

use std::collections::HashMap;

/// Documentation for a config section.
pub struct SectionDoc {
    /// TOML section name (e.g., "shell", "agents")
    pub name: &'static str,
    /// Human-readable description of the section
    pub description: &'static str,
    /// Fields in this section
    pub fields: &'static [FieldDoc],
}

/// Documentation for a config field.
pub struct FieldDoc {
    /// Field name as it appears in TOML
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Default value as a display string
    pub default_display: &'static str,
}

/// Config sections in canonical display order.
pub const CONFIG_SECTIONS: &[SectionDoc] = &[
    SectionDoc {
        name: "shell",
        description: "Shell integration settings",
        fields: &[FieldDoc {
            name: "auto_wrap",
            description: "Automatically wrap agent commands for recording",
            default_display: "true",
        }],
    },
    SectionDoc {
        name: "storage",
        description: "Storage and cleanup settings",
        fields: &[
            FieldDoc {
                name: "directory",
                description: "Base directory for session recordings",
                default_display: "~/recorded_agent_sessions",
            },
            FieldDoc {
                name: "size_threshold_gb",
                description: "Storage warning threshold in GB",
                default_display: "5.0",
            },
            FieldDoc {
                name: "age_threshold_days",
                description: "Age threshold in days for cleanup suggestions",
                default_display: "30",
            },
        ],
    },
    SectionDoc {
        name: "recording",
        description: "Recording behavior settings",
        fields: &[
            FieldDoc {
                name: "auto_analyze",
                description: "Automatically run AI analysis after recording ends",
                default_display: "false",
            },
            FieldDoc {
                name: "filename_template",
                description: "Filename template using {directory}, {date}, {time} tags",
                default_display: "{directory}_{date}_{time}",
            },
            FieldDoc {
                name: "directory_max_length",
                description: "Maximum characters for directory component in filename",
                default_display: "14",
            },
        ],
    },
    SectionDoc {
        name: "analysis",
        description: "AI analysis settings",
        fields: &[
            FieldDoc {
                name: "agent",
                description: "Preferred agent for analysis (claude, codex, gemini)",
                default_display: "auto-detect",
            },
            FieldDoc {
                name: "workers",
                description: "Number of parallel analysis workers (auto-scale if unset)",
                default_display: "auto",
            },
            FieldDoc {
                name: "timeout",
                description: "Timeout per analysis chunk in seconds",
                default_display: "120",
            },
            FieldDoc {
                name: "fast",
                description: "Fast mode: skip JSON schema enforcement",
                default_display: "false",
            },
            FieldDoc {
                name: "curate",
                description: "Auto-curate markers when count exceeds threshold",
                default_display: "true",
            },
        ],
    },
    SectionDoc {
        name: "agents",
        description: "Agent configuration",
        fields: &[
            FieldDoc {
                name: "enabled",
                description: "List of agents to auto-wrap",
                default_display: r#"["claude", "codex", "gemini"]"#,
            },
            FieldDoc {
                name: "no_wrap",
                description: "Agents to exclude from auto-wrapping",
                default_display: "[]",
            },
        ],
    },
];

/// Per-agent sub-section fields (applies to [agents.claude], [agents.codex], etc.)
pub const AGENT_FIELDS: &[FieldDoc] = &[
    FieldDoc {
        name: "extra_args",
        description: "Default extra CLI arguments for all tasks",
        default_display: "[]",
    },
    FieldDoc {
        name: "analyze_extra_args",
        description: "Extra CLI arguments for analysis (overrides extra_args)",
        default_display: "[]",
    },
    FieldDoc {
        name: "curate_extra_args",
        description: "Extra CLI arguments for curation (overrides extra_args)",
        default_display: "[]",
    },
    FieldDoc {
        name: "rename_extra_args",
        description: "Extra CLI arguments for rename (overrides extra_args)",
        default_display: "[]",
    },
    FieldDoc {
        name: "token_budget",
        description: "Override the token budget for this agent",
        default_display: "auto",
    },
];

/// Insert commented-out template lines for optional fields that are absent.
///
/// Scans the TOML string for known sections and appends `# field = example`
/// lines for any documented fields not already present. This lets users
/// discover all available options even when their default is "unset".
pub fn insert_optional_field_templates(toml_str: &str) -> String {
    let mut lines: Vec<String> = toml_str.lines().map(|l| l.to_string()).collect();

    // Collect present fields per section
    let mut present: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_section = String::new();
    for line in &lines {
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
            // Detect both `key = value` and `# key = value` (commented-out template)
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

    // For each section, find the last line that belongs to it and append missing fields
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

        // Find the last line of this section (before next section or EOF)
        let header = format!("[{}]", section_name);
        let section_start = lines.iter().position(|l| l.trim() == header);
        if let Some(start) = section_start {
            let section_end = lines[start + 1..]
                .iter()
                .position(|l| {
                    let t = l.trim();
                    t.starts_with('[') && !t.starts_with("[[")
                })
                .map(|i| start + 1 + i)
                .unwrap_or(lines.len());

            // Find last non-blank content line in this section so templates
            // appear right after the section's fields, not before the next header.
            let mut last_content = start;
            for (i, line) in lines.iter().enumerate().take(section_end).skip(start + 1) {
                if !line.trim().is_empty() {
                    last_content = i;
                }
            }
            let insert_at = last_content + 1;

            let mut templates: Vec<String> = Vec::new();
            for field in &missing {
                templates.push(format!("# {} = {}", field.name, field.default_display));
            }
            for (i, tmpl) in templates.into_iter().enumerate() {
                lines.insert(insert_at + i, tmpl);
            }
        }
    }

    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

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

/// Generate the Configuration wiki page as markdown.
pub fn generate_config_markdown() -> String {
    let mut md = String::new();

    md.push_str(
        "<!-- This file is auto-generated by `cargo xtask gen-docs`. Do not edit manually. -->\n\n",
    );
    md.push_str("# Configuration\n\n");
    md.push_str("AGR uses a TOML configuration file at `~/.config/agr/config.toml`.\n\n");
    md.push_str("## Quick Commands\n\n");
    md.push_str("```bash\n");
    md.push_str("agr config show      # View current configuration\n");
    md.push_str("agr config edit      # Open in your editor\n");
    md.push_str("agr config migrate   # Migrate to latest schema\n");
    md.push_str("```\n\n");
    md.push_str("## Configuration Sections\n\n");

    for section in CONFIG_SECTIONS {
        md.push_str(&format!("### [{}]\n\n", section.name));
        md.push_str(&format!("{}\n\n", section.description));
        md.push_str("| Option | Default | Description |\n");
        md.push_str("|--------|---------|-------------|\n");
        for field in section.fields {
            md.push_str(&format!(
                "| `{}` | `{}` | {} |\n",
                field.name, field.default_display, field.description
            ));
        }
        md.push('\n');
    }

    // Per-agent config
    md.push_str("### [agents.\\<name\\>]\n\n");
    md.push_str("Per-agent analysis configuration. Applies to `[agents.claude]`, `[agents.codex]`, `[agents.gemini]`.\n\n");
    md.push_str("| Option | Default | Description |\n");
    md.push_str("|--------|---------|-------------|\n");
    for field in AGENT_FIELDS {
        md.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            field.name, field.default_display, field.description
        ));
    }
    md.push('\n');

    // Filename templates (static reference content)
    md.push_str("## Filename Templates\n\n");
    md.push_str("Customize how recording filenames are generated using template tags.\n\n");
    md.push_str("### Available Tags\n\n");
    md.push_str("| Tag | Description | Example Output |\n");
    md.push_str("|-----|-------------|----------------|\n");
    md.push_str("| `{directory}` | Current working directory name | `my-project` |\n");
    md.push_str("| `{date}` | Date in YYMMDD format | `260129` |\n");
    md.push_str(
        "| `{date:FORMAT}` | Date with custom strftime | `{date:%Y-%m-%d}` → `2026-01-29` |\n",
    );
    md.push_str("| `{time}` | Time in HHMM format | `1430` |\n");
    md.push_str("| `{time:FORMAT}` | Time with custom strftime | `{time:%H:%M}` → `14:30` |\n");
    md.push('\n');

    md.push_str("### Example Templates\n\n");
    md.push_str("```toml\n");
    md.push_str("[recording]\n");
    md.push_str("# Default: project_260129_1430.cast\n");
    md.push_str("filename_template = \"{directory}_{date}_{time}\"\n\n");
    md.push_str("# ISO date: project_2026-01-29.cast\n");
    md.push_str("filename_template = \"{directory}_{date:%Y-%m-%d}\"\n\n");
    md.push_str("# Simple timestamp: 260129-143022.cast\n");
    md.push_str("filename_template = \"{date:%y%m%d}-{time:%H%M%S}\"\n");
    md.push_str("```\n\n");

    md.push_str("### Sanitization\n\n");
    md.push_str("Directory names are automatically sanitized:\n");
    md.push_str("- Spaces → hyphens\n");
    md.push_str("- Invalid characters removed (`/\\:*?\"<>|`)\n");
    md.push_str("- Unicode → ASCII transliteration\n");
    md.push_str("- Truncated to `directory_max_length`\n");
    md.push_str("- Windows reserved names handled (CON, NUL, etc.)\n\n");

    // Example config
    md.push_str("## Example Configuration\n\n");
    md.push_str("```toml\n");
    md.push_str("[shell]\n");
    md.push_str("auto_wrap = true\n\n");
    md.push_str("[storage]\n");
    md.push_str("directory = \"~/recorded_agent_sessions\"\n");
    md.push_str("size_threshold_gb = 10.0\n");
    md.push_str("age_threshold_days = 14\n\n");
    md.push_str("[recording]\n");
    md.push_str("auto_analyze = false\n");
    md.push_str("filename_template = \"{directory}_{date:%Y%m%d}_{time:%H%M}\"\n");
    md.push_str("directory_max_length = 40\n\n");
    md.push_str("[analysis]\n");
    md.push_str("agent = \"claude\"\n");
    md.push_str("timeout = 120\n\n");
    md.push_str("[agents]\n");
    md.push_str("enabled = [\"claude\", \"codex\", \"gemini\"]\n");
    md.push_str("no_wrap = [\"gemini\"]\n\n");
    md.push_str("[agents.codex]\n");
    md.push_str("extra_args = [\"--model\", \"o3\"]\n");
    md.push_str("```\n");

    md
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

    #[test]
    fn generate_markdown_includes_all_sections() {
        let md = generate_config_markdown();
        for section in CONFIG_SECTIONS {
            assert!(
                md.contains(&format!("### [{}]", section.name)),
                "missing section: {}",
                section.name
            );
            for field in section.fields {
                assert!(
                    md.contains(field.name),
                    "missing field: {}.{}",
                    section.name,
                    field.name
                );
            }
        }
        assert!(md.contains("[agents.\\<name\\>]"));
    }

    #[test]
    fn config_sections_match_canonical_order() {
        let names: Vec<&str> = CONFIG_SECTIONS.iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            vec!["shell", "storage", "recording", "analysis", "agents"]
        );
    }
}
