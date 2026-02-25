//! Config subcommands handler

use anyhow::{Context, Result};
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

use agr::config::{migrate_config, MigrateResult};
use agr::theme::ansi;
use agr::theme::{current_theme, Theme};
use agr::Config;

/// Show current configuration as TOML with inline documentation comments.
#[cfg(not(tarpaulin_include))]
pub fn handle_show() -> Result<()> {
    let config = Config::load()?;
    let toml_str = toml::to_string_pretty(&config)?;
    // Insert commented-out templates for optional fields (e.g. # workers = auto)
    // before annotation so they also get documentation comments
    let with_templates = agr::config::docs::insert_optional_field_templates(&toml_str);
    let annotated = agr::config::docs::annotate_config(&with_templates);
    let theme = current_theme();
    println!("{}", theme.primary_text(&annotated));
    Ok(())
}

/// Open configuration file in the default editor.
///
/// Uses $EDITOR environment variable (defaults to 'vi').
#[cfg(not(tarpaulin_include))]
pub fn handle_edit() -> Result<()> {
    let config_path = Config::config_path()?;
    let theme = current_theme();

    // Ensure config exists
    if !config_path.exists() {
        let config = Config::default();
        config.save()?;
    }

    // Get editor from environment
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    println!(
        "{}",
        theme.primary_text(&format!(
            "Opening {} with {}",
            config_path.display(),
            editor
        ))
    );

    std::process::Command::new(&editor)
        .arg(&config_path)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to open editor: {}", e))?;

    Ok(())
}

/// Migrate config file to the latest schema version.
///
/// Reads the existing config file (or empty if it doesn't exist),
/// runs versioned migrations, adds any missing fields from defaults,
/// shows a preview of changes, and prompts for confirmation.
///
/// # Arguments
/// * `auto_confirm` - If true, skip confirmation prompt (for --yes flag)
#[cfg(not(tarpaulin_include))]
pub fn handle_migrate(auto_confirm: bool) -> Result<()> {
    let theme = current_theme();
    let config_path = Config::config_path()?;
    let file_exists = config_path.exists();

    // Read existing content (empty string if file doesn't exist)
    let content = if file_exists {
        fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    // Run migration
    let result = migrate_config(&content)?;

    // Case 1: No changes needed
    if !result.has_changes() {
        println!("{}", theme.primary_text("Config is already up to date."));
        return Ok(());
    }

    // Case 2: Config file doesn't exist - offer to create with full defaults
    if !file_exists {
        return handle_new_config_creation(&config_path, &result, auto_confirm, &theme);
    }

    // Case 3: Config exists but needs changes - show diff and confirm
    print_migration_info(&result, &theme);
    print_diff_preview(&result.content, &result.added_fields, false);
    println!();

    // Prompt for confirmation (or auto-confirm with --yes)
    if !should_proceed(
        &format!("Apply these changes to {}?", config_path.display()),
        auto_confirm,
    )? {
        println!("{}", theme.primary_text("No changes made."));
        return Ok(());
    }

    // Write the updated config atomically
    atomic_write(&config_path, &result.content)?;
    println!("{}", theme.success_text("Config updated successfully."));

    Ok(())
}

/// Handle the case where no config file exists: show preview and create it.
#[cfg(not(tarpaulin_include))]
fn handle_new_config_creation(
    config_path: &std::path::Path,
    result: &MigrateResult,
    auto_confirm: bool,
    theme: &Theme,
) -> Result<()> {
    println!(
        "{}",
        theme.primary_text("Config file does not exist. Will create with default settings.")
    );
    println!();
    print_diff_preview(&result.content, &result.added_fields, true);
    println!();

    if !should_proceed(&format!("Create {}?", config_path.display()), auto_confirm)? {
        println!("{}", theme.primary_text("No changes made."));
        return Ok(());
    }

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(config_path, &result.content)?;
    println!(
        "{}",
        theme.success_text("Config file created successfully.")
    );
    Ok(())
}

/// Print version info, removed fields, and added fields summary for an existing config update.
fn print_migration_info(result: &MigrateResult, theme: &Theme) {
    if result.old_version != result.new_version {
        println!(
            "{}",
            theme.primary_text(&format!(
                "Migrating config from v{} to v{}",
                result.old_version, result.new_version
            ))
        );
    }

    if !result.removed_fields.is_empty() {
        println!(
            "{}",
            theme.primary_text(&format!(
                "Removed/moved {} deprecated field(s):",
                result.removed_fields.len()
            ))
        );
        for field in &result.removed_fields {
            println!("{}  - {}{}", ansi::RED, field, ansi::RESET);
        }
    }

    let total_fields = result.added_fields.len();
    let total_sections = result.sections_added.len();
    if total_fields > 0 {
        if total_sections > 0 {
            println!(
                "{}",
                theme.primary_text(&format!(
                    "Adding {} missing field(s) in {} new section(s):",
                    total_fields, total_sections
                ))
            );
        } else {
            println!(
                "{}",
                theme.primary_text(&format!("Adding {} missing field(s):", total_fields))
            );
        }
    }
    println!();
}

/// Reset config to defaults, backing up the current file.
#[cfg(not(tarpaulin_include))]
pub fn handle_reset(auto_confirm: bool) -> Result<()> {
    let theme = current_theme();
    let config_path = Config::config_path()?;

    // Generate fresh default config through the migration pipeline.
    // Using migrate_config("") ensures the reset config has the same structure,
    // section ordering, and commented-out templates as a freshly migrated config.
    let result = migrate_config("")?;

    if !config_path.exists() {
        println!(
            "{}",
            theme.primary_text("No config file exists. Creating with default settings.")
        );
    } else {
        println!(
            "{}",
            theme.primary_text("This will replace your config with default settings.")
        );
    }

    if !should_proceed("Reset configuration to defaults?", auto_confirm)? {
        println!("{}", theme.primary_text("No changes made."));
        return Ok(());
    }

    // Back up existing config (use numbered suffix to avoid overwriting previous backups)
    if config_path.exists() {
        let mut backup_path = config_path.with_extension("toml.bak");
        let mut counter = 1u32;
        while backup_path.exists() {
            backup_path = config_path.with_extension(format!("toml.bak.{}", counter));
            counter += 1;
        }
        fs::copy(&config_path, &backup_path)
            .with_context(|| format!("Failed to back up config to {}", backup_path.display()))?;
        println!(
            "{}",
            theme.secondary_text(&format!("Backed up to {}", backup_path.display()))
        );
    }

    // Write fresh default config
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write(&config_path, &result.content)?;
    println!("{}", theme.success_text("Config reset to defaults."));

    Ok(())
}

/// Print a diff-style preview of the config changes.
///
/// Shows lines that contain added fields with a green `+` prefix.
/// For new files, shows all content as additions.
fn print_diff_preview(new_content: &str, added_fields: &[String], is_new_file: bool) {
    // Build a set of full field paths (section.key) for accurate matching
    let added_field_set: std::collections::HashSet<&str> =
        added_fields.iter().map(|s| s.as_str()).collect();

    let mut current_section = String::new();
    let mut section_has_additions = false;
    let mut pending_section_header: Option<String> = None;

    for line in new_content.lines() {
        let trimmed = line.trim();

        // Track section headers - handle standard [section], but skip [[arrays]] and [a.b.c] dotted
        if let Some(section_name) = parse_simple_section_header(trimmed) {
            handle_section_header_line(
                line,
                section_name,
                added_fields,
                is_new_file,
                &mut current_section,
                &mut section_has_additions,
                &mut pending_section_header,
            );
            continue;
        }

        // Check if this line is a field assignment
        if trimmed.contains('=') {
            print_field_line(
                line,
                trimmed,
                &current_section,
                &added_field_set,
                is_new_file,
                section_has_additions,
                &mut pending_section_header,
            );
        } else if is_new_file && !trimmed.is_empty() {
            // For new files, show comments too
            if let Some(header) = pending_section_header.take() {
                println!("{}+{} {}{}", ansi::GREEN, ansi::RESET, ansi::GREEN, header);
            }
            println!("{}+ {}{}", ansi::GREEN, line, ansi::RESET);
        }
    }
}

/// Update state for a section header line and queue it for deferred printing.
fn handle_section_header_line(
    line: &str,
    section_name: &str,
    added_fields: &[String],
    is_new_file: bool,
    current_section: &mut String,
    section_has_additions: &mut bool,
    pending_section_header: &mut Option<String>,
) {
    let is_added_section = added_fields
        .iter()
        .any(|f| f.starts_with(&format!("{}.", section_name)));

    *current_section = section_name.to_string();
    *section_has_additions = is_added_section;

    if is_new_file || is_added_section {
        *pending_section_header = Some(line.to_string());
    } else {
        *pending_section_header = None;
    }
}

/// Print a field assignment line, with green prefix if it is added or the file is new.
fn print_field_line(
    line: &str,
    trimmed: &str,
    current_section: &str,
    added_field_set: &std::collections::HashSet<&str>,
    is_new_file: bool,
    section_has_additions: bool,
    pending_section_header: &mut Option<String>,
) {
    let eq_pos = match trimmed.find('=') {
        Some(pos) => pos,
        None => return,
    };
    let key = trimmed[..eq_pos].trim();
    let full_path = if current_section.is_empty() {
        key.to_string()
    } else {
        format!("{}.{}", current_section, key)
    };
    let is_added = added_field_set.contains(full_path.as_str());

    if is_new_file || is_added {
        if let Some(header) = pending_section_header.take() {
            println!("{}+{} {}{}", ansi::GREEN, ansi::RESET, ansi::GREEN, header);
        }
        println!("{}+ {}{}", ansi::GREEN, line, ansi::RESET);
    } else if section_has_additions {
        // Show context: flush pending section header once we know there are additions
        if let Some(header) = pending_section_header.take() {
            println!("  {}", header);
        }
        // Skip showing existing fields to keep diff focused
    }
}

/// Parse a simple TOML section header like `[section]`.
///
/// Returns None for:
/// - Array of tables: `[[array]]`
/// - Dotted keys: `[a.b.c]`
/// - Non-section lines
fn parse_simple_section_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();

    // Must start with [ and end with ]
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }

    // Skip array of tables [[...]]
    if trimmed.starts_with("[[") {
        return None;
    }

    // Extract content between brackets
    let inner = &trimmed[1..trimmed.len() - 1];

    // Skip dotted section names like [a.b.c] - we only handle simple [section]
    if inner.contains('.') {
        return None;
    }

    // Skip empty section names
    if inner.trim().is_empty() {
        return None;
    }

    Some(inner.trim())
}

/// Determine whether to proceed with the operation.
///
/// If `auto_confirm` is true (--yes flag), returns true immediately.
/// Otherwise, prompts the user for confirmation.
/// If stdin is not a TTY, returns false with a hint about --yes.
fn should_proceed(message: &str, auto_confirm: bool) -> Result<bool> {
    if auto_confirm {
        return Ok(true);
    }

    let theme = current_theme();

    // Check if stdin is a TTY - if not, skip prompt and return false
    if !atty::is(atty::Stream::Stdin) {
        println!(
            "{}",
            theme.secondary_text("Non-interactive mode: use --yes to apply changes automatically")
        );
        return Ok(false);
    }

    print!("{} [y/N] ", theme.primary_text(message));
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;

    let response = input.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}

/// Write content to a file atomically.
///
/// Writes to a temporary file first, then renames to the target path.
/// This prevents corruption if the write is interrupted.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    // Create temp file in the same directory (ensures same filesystem for rename)
    let parent = path
        .parent()
        .context("Config path has no parent directory")?;
    let temp_path = parent.join(format!(
        ".{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config")
    ));

    // Write to temp file
    fs::write(&temp_path, content)
        .with_context(|| format!("Failed to write temp file: {:?}", temp_path))?;

    // Rename temp to target (atomic on most filesystems)
    fs::rename(&temp_path, path).with_context(|| {
        // Clean up temp file on rename failure
        let _ = fs::remove_file(&temp_path);
        format!("Failed to rename {:?} to {:?}", temp_path, path)
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- handle_section_header_line ----

    #[test]
    fn handle_section_header_with_additions_queues_pending_header() {
        let mut current_section = String::new();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[analysis]",
            "analysis",
            &["analysis.workers".to_string()],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        assert_eq!(current_section, "analysis");
        assert!(section_has_additions);
        assert_eq!(pending, Some("[analysis]".to_string()));
    }

    #[test]
    fn handle_section_header_no_additions_clears_pending() {
        let mut current_section = "old".to_string();
        let mut section_has_additions = true;
        let mut pending: Option<String> = Some("[old]".to_string());

        handle_section_header_line(
            "[recording]",
            "recording",
            &["analysis.workers".to_string()],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        assert_eq!(current_section, "recording");
        assert!(!section_has_additions);
        assert_eq!(pending, None);
    }

    #[test]
    fn handle_section_header_new_file_always_queues() {
        let mut current_section = String::new();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[shell]",
            "shell",
            &[],  // no additions
            true, // is_new_file = true
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        // new file => pending header is set even without additions
        assert_eq!(pending, Some("[shell]".to_string()));
    }

    #[test]
    fn handle_section_header_always_updates_current_section() {
        let mut current_section = "old_section".to_string();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[storage]",
            "storage",
            &[],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        assert_eq!(current_section, "storage");
    }

    #[test]
    fn handle_section_header_prefix_no_false_positive() {
        // "anal" is a prefix of "analysis" but should NOT match "analysis.workers"
        let mut current_section = String::new();
        let mut section_has_additions = false;
        let mut pending: Option<String> = None;

        handle_section_header_line(
            "[anal]",
            "anal",
            &["analysis.workers".to_string()],
            false,
            &mut current_section,
            &mut section_has_additions,
            &mut pending,
        );

        // "anal.workers" is not an entry in added_fields, so no additions
        assert!(!section_has_additions);
        assert_eq!(pending, None);
    }

    // ---- parse_simple_section_header ----

    #[test]
    fn parse_simple_section_header_valid() {
        assert_eq!(parse_simple_section_header("[analysis]"), Some("analysis"));
    }

    #[test]
    fn parse_simple_section_header_array_of_tables_returns_none() {
        assert_eq!(parse_simple_section_header("[[agents]]"), None);
    }

    #[test]
    fn parse_simple_section_header_dotted_returns_none() {
        assert_eq!(parse_simple_section_header("[a.b.c]"), None);
    }

    #[test]
    fn parse_simple_section_header_not_a_section_returns_none() {
        assert_eq!(parse_simple_section_header("workers = 4"), None);
    }

    #[test]
    fn parse_simple_section_header_empty_brackets_returns_none() {
        assert_eq!(parse_simple_section_header("[]"), None);
    }

    #[test]
    fn parse_simple_section_header_trims_whitespace() {
        assert_eq!(
            parse_simple_section_header("[ recording ]"),
            Some("recording")
        );
    }
}
