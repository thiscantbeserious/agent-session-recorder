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
pub const CURRENT_VERSION: u32 = 5;

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

    let preamble_end = headers[0].0;
    let preamble = lines[..preamble_end].join("\n");

    let blocks = collect_section_blocks(&headers, &lines);
    let sorted_blocks = sort_blocks_by_order(&blocks);

    reassemble_toml(&preamble, &sorted_blocks)
}

/// Build section blocks from headers, merging sub-sections into their parent group.
///
/// Returns `Vec<(group_name, block_text)>` where non-consecutive sections with
/// the same top-level group are concatenated into a single block.
fn collect_section_blocks(headers: &[(usize, String)], lines: &[&str]) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut seen_groups: Vec<String> = Vec::new();

    for (idx, (start, group)) in headers.iter().enumerate() {
        let end = if idx + 1 < headers.len() {
            headers[idx + 1].0
        } else {
            lines.len()
        };

        let block_text = lines[*start..end].join("\n");

        if let Some(pos) = seen_groups.iter().position(|g| g == group) {
            blocks[pos].1.push('\n');
            blocks[pos].1.push_str(&block_text);
        } else {
            seen_groups.push(group.clone());
            blocks.push((group.clone(), block_text));
        }
    }

    blocks
}

/// Sort blocks so that known sections appear in `SECTION_ORDER`, unknowns follow in original order.
fn sort_blocks_by_order(blocks: &[(String, String)]) -> Vec<&(String, String)> {
    let mut sorted: Vec<&(String, String)> = Vec::new();
    for &section in SECTION_ORDER {
        if let Some(block) = blocks.iter().find(|(g, _)| g == section) {
            sorted.push(block);
        }
    }
    for block in blocks {
        if !SECTION_ORDER.contains(&block.0.as_str()) {
            sorted.push(block);
        }
    }
    sorted
}

/// Reassemble preamble and sorted blocks into a single TOML string.
///
/// Ensures exactly one blank line between adjacent blocks and a trailing newline.
fn reassemble_toml(preamble: &str, sorted_blocks: &[&(String, String)]) -> String {
    let mut result = String::new();

    if !preamble.is_empty() {
        result.push_str(preamble);
        if !result.ends_with('\n') {
            result.push('\n');
        }
    }

    for (i, block) in sorted_blocks.iter().enumerate() {
        if i > 0 || !preamble.is_empty() {
            ensure_blank_line_separator(&mut result);
        }
        result.push_str(&block.1);
        if !result.ends_with('\n') {
            result.push('\n');
        }
    }

    result
}

/// Enforce the blank-line separator invariant: `result` ends with exactly two newlines.
///
/// Trims any excess trailing newlines down to one, then appends one more so the
/// result always ends with `\n\n` (one blank line before the next block).
fn ensure_blank_line_separator(result: &mut String) {
    let trimmed = result.trim_end_matches('\n');
    let new_len = trimmed.len();
    result.truncate(new_len);
    result.push_str("\n\n");
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
mod tests;
