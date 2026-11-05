//! Deletion prompt, input parsing and confirmed delete for the text cleanup.

use std::io::{self, BufRead, Write};

use anyhow::Result;

use agr::storage::SessionInfo;
use agr::theme::current_theme;
use agr::StorageManager;

/// Process user input and perform deletion.
pub(super) fn process_deletion_input(
    sessions: &[SessionInfo],
    old_count: usize,
    age_threshold: u32,
    storage: &StorageManager,
) -> Result<()> {
    let theme = current_theme();
    // Build prompt with quick delete options
    let prompt = if old_count > 0 {
        format!(
            "Delete: [number], 'old' ({} sessions > {}d), 'all', or 0 to cancel: ",
            old_count, age_threshold
        )
    } else {
        "Delete: [number], 'all', or 0 to cancel: ".to_string()
    };
    print!("{}", theme.primary_text(&prompt));
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().lock().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    // Parse input - could be number, 'old', or 'all'
    let to_delete = parse_deletion_input(&input, sessions, old_count, age_threshold)?;

    if to_delete.is_empty() {
        return Ok(());
    }

    // Confirm and execute deletion
    confirm_and_delete(&to_delete, storage)
}

/// Parse user input and return sessions to delete.
pub(super) fn parse_deletion_input(
    input: &str,
    sessions: &[SessionInfo],
    old_count: usize,
    age_threshold: u32,
) -> Result<Vec<SessionInfo>> {
    let theme = current_theme();
    if input == "0" || input.is_empty() {
        println!("{}", theme.primary_text("No sessions deleted."));
        return Ok(vec![]);
    } else if input == "all" {
        return Ok(sessions.to_vec());
    } else if input == "old" && old_count > 0 {
        return Ok(sessions
            .iter()
            .filter(|s| s.age_days > age_threshold as i64)
            .cloned()
            .collect());
    } else if let Ok(count) = input.parse::<usize>() {
        if count > sessions.len() {
            println!(
                "{}",
                theme.primary_text(&format!("Invalid number. Maximum is {}.", sessions.len()))
            );
            return Ok(vec![]);
        }
        return Ok(sessions.iter().take(count).cloned().collect());
    }

    println!(
        "{}",
        theme.primary_text("Invalid input. Use a number, 'old', 'all', or 0 to cancel.")
    );
    Ok(vec![])
}

/// Confirm deletion with user and execute.
fn confirm_and_delete(to_delete: &[SessionInfo], storage: &StorageManager) -> Result<()> {
    let theme = current_theme();
    // Calculate total size to be freed
    let total_size: u64 = to_delete.iter().map(|s| s.size).sum();

    println!();
    println!(
        "{}",
        theme.primary_text(&format!(
            "Will delete {} sessions ({}):",
            to_delete.len(),
            humansize::format_size(total_size, humansize::BINARY)
        ))
    );
    for session in to_delete.iter().take(10) {
        println!(
            "{}",
            theme.primary_text(&format!("  - {} ({})", session.filename, session.agent))
        );
    }
    if to_delete.len() > 10 {
        println!(
            "{}",
            theme.primary_text(&format!("  ... and {} more", to_delete.len() - 10))
        );
    }

    print!("{}", theme.primary_text("\nConfirm? [y/N]: "));
    io::stdout().flush()?;

    let mut confirm = String::new();
    io::stdin().lock().read_line(&mut confirm)?;

    if confirm.trim().to_lowercase() == "y" {
        let freed = storage.delete_sessions(to_delete)?;
        let new_stats = storage.get_stats()?;
        println!(
            "{}",
            theme.primary_text(&format!(
                "Deleted {} sessions (freed {}). New size: {}",
                to_delete.len(),
                humansize::format_size(freed, humansize::BINARY),
                new_stats.size_human()
            ))
        );
    } else {
        println!("{}", theme.primary_text("Cancelled."));
    }

    Ok(())
}
