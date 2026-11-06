//! Config subcommands handler

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

mod diff;
use diff::print_diff_preview;

use super::prompt::should_proceed;

use agr::config::{migrate_config, MigrateResult};
use agr::theme::{ansi, gate};
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
            println!("{}  - {}{}", gate(ansi::RED), field, gate(ansi::RESET));
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
