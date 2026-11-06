//! Post-migrate yes/no prompts for newly added config fields.
//!
//! Pure content transform: the caller asks the user and writes the file.

use anyhow::{Context, Result};
use toml_edit::{DocumentMut, Item, Value};

use super::MigrateResult;

/// A question asked once after a field was added by migration.
pub struct PostMigratePrompt {
    pub key: &'static str,
    pub question: &'static str,
    pub default: bool,
}

pub const PROMPTS: &[PostMigratePrompt] = &[PostMigratePrompt {
    key: "recording.auto_optimize",
    question: "Optimize recordings automatically after each session?",
    default: true,
}];

/// Run the built-in prompts against a migrate result.
pub fn apply_prompts(
    result: &MigrateResult,
    ask: impl FnMut(&PostMigratePrompt) -> Result<bool>,
) -> Result<String> {
    apply_prompt_list(result, PROMPTS, ask)
}

/// Ask each prompt whose key was added and return the updated content.
pub fn apply_prompt_list(
    result: &MigrateResult,
    prompts: &[PostMigratePrompt],
    mut ask: impl FnMut(&PostMigratePrompt) -> Result<bool>,
) -> Result<String> {
    let mut doc: DocumentMut = result.content.parse().context("Failed to parse config")?;
    let mut changed = false;
    for prompt in prompts
        .iter()
        .filter(|p| result.added_fields.iter().any(|f| f == p.key))
    {
        let answer = ask(prompt)?;
        changed |= set_dotted_bool(&mut doc, prompt.key, answer);
    }
    // Untouched content is returned verbatim, so nothing is reformatted.
    Ok(if changed {
        doc.to_string()
    } else {
        result.content.clone()
    })
}

/// Walk a dotted key through tables and set the leaf to a bool.
/// Returns false if the path does not exist or the value already matches.
fn set_dotted_bool(doc: &mut DocumentMut, key: &str, val: bool) -> bool {
    let mut parts: Vec<&str> = key.split('.').collect();
    let Some(leaf) = parts.pop() else {
        return false;
    };
    let mut table = doc.as_table_mut();
    for part in parts {
        match table.get_mut(part).and_then(Item::as_table_mut) {
            Some(next) => table = next,
            None => return false,
        }
    }
    if table.get(leaf).and_then(Item::as_bool) == Some(val) {
        return false;
    }
    // Keep the inline comment and spacing of an existing value.
    let decor = table
        .get(leaf)
        .and_then(Item::as_value)
        .map(|v| v.decor().clone());
    let mut new = Value::from(val);
    if let Some(d) = decor {
        *new.decor_mut() = d;
    }
    table.insert(leaf, Item::Value(new));
    true
}
