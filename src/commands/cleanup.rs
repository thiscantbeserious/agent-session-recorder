//! Cleanup command handler

use anyhow::Result;
use std::io::{self, BufRead, Write};

use agr::storage::{SessionInfo, StorageStats};
use agr::{Config, StorageManager};

use super::truncate_string;

/// Interactive cleanup of old session recordings.
///
/// Displays sessions sorted by age and lets you choose how many to delete.
/// Supports filtering by agent and age threshold.
#[cfg(not(tarpaulin_include))]
pub fn handle(agent_filter: Option<&str>, older_than: Option<u32>) -> Result<()> {
    let config = Config::load()?;
    let age_threshold = config.storage.age_threshold_days;
    let storage = StorageManager::new(config);

    // Get sessions, optionally filtered by agent
    let mut sessions = storage.list_sessions(agent_filter)?;

    // Apply older_than filter if specified
    if let Some(days) = older_than {
        sessions.retain(|s| s.age_days > days as i64);
    }

    if sessions.is_empty() {
        if agent_filter.is_some() || older_than.is_some() {
            println!("No sessions match the specified filters.");
        } else {
            println!("No sessions to clean up.");
        }
        return Ok(());
    }

    let stats = storage.get_stats()?;

    // Count old sessions (older than configured threshold)
    let old_count = sessions
        .iter()
        .filter(|s| s.age_days > age_threshold as i64)
        .count();

    // Print header with breakdown by agent
    print_header(&stats, agent_filter, older_than)?;

    // Build session summary message
    print_session_summary(sessions.len(), old_count, age_threshold);

    // Print formatted table
    print_sessions_table(&sessions, age_threshold);

    // Get user input and process deletion
    process_deletion_input(&sessions, old_count, age_threshold, &storage)
}

/// Print the cleanup header with storage info and filters.
fn print_header(
    stats: &StorageStats,
    agent_filter: Option<&str>,
    older_than: Option<u32>,
) -> Result<()> {
    println!("=== Agent Session Cleanup ===");
    println!(
        "Storage: {} ({:.1}% of disk)",
        stats.size_human(),
        stats.disk_percentage
    );

    // Show agent breakdown
    let agents_summary: Vec<String> = stats
        .sessions_by_agent
        .iter()
        .map(|(agent, count)| format!("{}: {}", agent, count))
        .collect();
    if !agents_summary.is_empty() {
        println!(
            "   Sessions: {} total ({})",
            stats.session_count,
            agents_summary.join(", ")
        );
    }
    println!();

    // Show filter info if applicable
    if let Some(agent) = agent_filter {
        println!("Filtered by agent: {}", agent);
    }
    if let Some(days) = older_than {
        println!("Filtered by age: > {} days", days);
    }
    if agent_filter.is_some() || older_than.is_some() {
        println!();
    }

    Ok(())
}

/// Print the session summary message.
fn print_session_summary(total: usize, old_count: usize, age_threshold: u32) {
    let session_msg = if old_count > 0 {
        format!(
            "Found {} sessions ({} older than {} days - marked with *)",
            total, old_count, age_threshold
        )
    } else {
        format!("Found {} sessions", total)
    };
    println!("{}", session_msg);
    println!();
}

/// Print the sessions table (up to 15 entries).
fn print_sessions_table(sessions: &[SessionInfo], age_threshold: u32) {
    println!("  #  |  Age   | DateTime         | Agent       | Size       | Filename");
    println!(
        "-----+--------+------------------+-------------+------------+---------------------------"
    );

    for (i, session) in sessions.iter().take(15).enumerate() {
        let age_marker = if session.age_days > age_threshold as i64 {
            "*"
        } else {
            " "
        };
        println!(
            "{:>3}  | {:>5}{} | {} | {:11} | {:>10} | {}",
            i + 1,
            session.format_age(),
            age_marker,
            session.modified.format("%Y-%m-%d %H:%M"),
            truncate_string(&session.agent, 11),
            session.size_human(),
            session.filename
        );
    }

    if sessions.len() > 15 {
        println!("... and {} more sessions", sessions.len() - 15);
    }
    println!();
}

/// Process user input and perform deletion.
fn process_deletion_input(
    sessions: &[SessionInfo],
    old_count: usize,
    age_threshold: u32,
    storage: &StorageManager,
) -> Result<()> {
    // Build prompt with quick delete options
    let prompt = if old_count > 0 {
        format!(
            "Delete: [number], 'old' ({} sessions > {}d), 'all', or 0 to cancel: ",
            old_count, age_threshold
        )
    } else {
        "Delete: [number], 'all', or 0 to cancel: ".to_string()
    };
    print!("{}", prompt);
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
fn parse_deletion_input(
    input: &str,
    sessions: &[SessionInfo],
    old_count: usize,
    age_threshold: u32,
) -> Result<Vec<SessionInfo>> {
    if input == "0" || input.is_empty() {
        println!("No sessions deleted.");
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
            println!("Invalid number. Maximum is {}.", sessions.len());
            return Ok(vec![]);
        }
        return Ok(sessions.iter().take(count).cloned().collect());
    }

    println!("Invalid input. Use a number, 'old', 'all', or 0 to cancel.");
    Ok(vec![])
}

/// Confirm deletion with user and execute.
fn confirm_and_delete(to_delete: &[SessionInfo], storage: &StorageManager) -> Result<()> {
    // Calculate total size to be freed
    let total_size: u64 = to_delete.iter().map(|s| s.size).sum();

    println!();
    println!(
        "Will delete {} sessions ({}):",
        to_delete.len(),
        humansize::format_size(total_size, humansize::BINARY)
    );
    for session in to_delete.iter().take(10) {
        println!("  - {} ({})", session.filename, session.agent);
    }
    if to_delete.len() > 10 {
        println!("  ... and {} more", to_delete.len() - 10);
    }

    print!("\nConfirm? [y/N]: ");
    io::stdout().flush()?;

    let mut confirm = String::new();
    io::stdin().lock().read_line(&mut confirm)?;

    if confirm.trim().to_lowercase() == "y" {
        let freed = storage.delete_sessions(to_delete)?;
        let new_stats = storage.get_stats()?;
        println!(
            "Deleted {} sessions (freed {}). New size: {}",
            to_delete.len(),
            humansize::format_size(freed, humansize::BINARY),
            new_stats.size_human()
        );
    } else {
        println!("Cancelled.");
    }

    Ok(())
}
