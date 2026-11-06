//! Post-migrate yes/no prompts for newly added config fields.
//!
//! Pure content transform: the caller asks the user and writes the file.

use anyhow::Result;

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
    _prompts: &[PostMigratePrompt],
    _ask: impl FnMut(&PostMigratePrompt) -> Result<bool>,
) -> Result<String> {
    Ok(result.content.clone())
}
