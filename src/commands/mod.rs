//! Command handlers for the AGR CLI.
//!
//! Each submodule handles a specific CLI command or command group.
//! The main dispatch logic remains in main.rs.

pub mod agents;
pub mod analyze;
pub mod cleanup;
pub mod completions;
pub mod config;
pub mod copy;
pub mod list;
pub mod marker;
pub mod play;
pub mod prompt;
pub mod record;
pub mod shell;
pub mod status;
pub mod transform;

/// Resolve `file` (absolute, short or fuzzy), fail if missing, warn if not `.cast` or `.cast.gz`.
pub fn resolve_existing_session(
    file: &str,
    config: &agr::Config,
) -> anyhow::Result<std::path::PathBuf> {
    let filepath = agr::files::resolve::resolve_file_path(file, config)?;
    if !filepath.exists() {
        anyhow::bail!(
            "File not found: {}\nHint: Use format 'agent/file.cast'. Run 'agr list' to see available sessions.",
            file
        );
    }
    if !agr::files::codec::is_session_path(&filepath) {
        eprintln!("Warning: File does not have .cast or .cast.gz extension");
    }
    Ok(filepath)
}

/// Truncate a string to a maximum length, adding ellipsis if needed.
pub fn truncate_string(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len > 3 {
        let truncated: String = s.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        s.chars().take(max_len).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_string_short_string_unchanged() {
        assert_eq!(truncate_string("hello", 10), "hello");
    }

    #[test]
    fn truncate_string_exact_length_unchanged() {
        assert_eq!(truncate_string("hello", 5), "hello");
    }

    #[test]
    fn truncate_string_long_string_with_ellipsis() {
        assert_eq!(truncate_string("hello world", 8), "hello...");
    }

    #[test]
    fn truncate_string_very_short_max_len() {
        // When max_len <= 3, just truncate without ellipsis
        assert_eq!(truncate_string("hello", 3), "hel");
    }

    #[test]
    fn truncate_string_empty_string() {
        assert_eq!(truncate_string("", 10), "");
    }

    #[test]
    fn truncate_string_handles_multibyte_characters() {
        // Should not panic and should truncate by characters, not bytes
        assert_eq!(truncate_string("日本語テスト", 5), "日本...");
        assert_eq!(truncate_string("café", 10), "café");
        assert_eq!(truncate_string("emoji🎉test", 8), "emoji...");
    }
}
