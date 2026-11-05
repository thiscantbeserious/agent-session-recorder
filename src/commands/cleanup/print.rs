//! Print helpers for the cleanup header, summary and session table.

use anyhow::Result;

use agr::storage::{SessionInfo, StorageStats};
use agr::theme::current_theme;

use super::super::truncate_string;

/// Print the cleanup header with storage info and filters.
pub(super) fn print_header(
    stats: &StorageStats,
    agent_filter: Option<&str>,
    older_than: Option<u32>,
) -> Result<()> {
    let theme = current_theme();
    println!("{}", theme.primary_text("=== Agent Session Cleanup ==="));
    println!(
        "{}",
        theme.primary_text(&format!(
            "Storage: {} ({:.1}% of disk)",
            stats.size_human(),
            stats.disk_percentage
        ))
    );

    // Show agent breakdown
    let agents_summary: Vec<String> = stats
        .sessions_by_agent
        .iter()
        .map(|(agent, count)| format!("{}: {}", agent, count))
        .collect();
    if !agents_summary.is_empty() {
        println!(
            "{}",
            theme.primary_text(&format!(
                "   Sessions: {} total ({})",
                stats.session_count,
                agents_summary.join(", ")
            ))
        );
    }
    println!();

    // Show filter info if applicable
    if let Some(agent) = agent_filter {
        println!(
            "{}",
            theme.primary_text(&format!("Filtered by agent: {}", agent))
        );
    }
    if let Some(days) = older_than {
        println!(
            "{}",
            theme.primary_text(&format!("Filtered by age: > {} days", days))
        );
    }
    if agent_filter.is_some() || older_than.is_some() {
        println!();
    }

    Ok(())
}

/// Print the session summary message.
pub(super) fn print_session_summary(total: usize, old_count: usize, age_threshold: u32) {
    let theme = current_theme();
    let session_msg = if old_count > 0 {
        format!(
            "Found {} sessions ({} older than {} days - marked with *)",
            total, old_count, age_threshold
        )
    } else {
        format!("Found {} sessions", total)
    };
    println!("{}", theme.primary_text(&session_msg));
    println!();
}

/// Print the sessions table (up to 15 entries).
pub(super) fn print_sessions_table(sessions: &[SessionInfo], age_threshold: u32) {
    let theme = current_theme();
    println!(
        "{}",
        theme
            .primary_text("  #  |  Age   | DateTime         | Agent       | Size       | Filename")
    );
    println!(
        "{}",
        theme.primary_text(
            "-----+--------+------------------+-------------+------------+---------------------------"
        )
    );

    for (i, session) in sessions.iter().take(15).enumerate() {
        let age_marker = if session.age_days > age_threshold as i64 {
            "*"
        } else {
            " "
        };
        println!(
            "{}",
            theme.primary_text(&format!(
                "{:>3}  | {:>5}{} | {} | {:11} | {:>10} | {}",
                i + 1,
                session.format_age(),
                age_marker,
                session.modified.format("%Y-%m-%d %H:%M"),
                truncate_string(&session.agent, 11),
                session.size_human(),
                session.filename
            ))
        );
    }

    if sessions.len() > 15 {
        println!(
            "{}",
            theme.primary_text(&format!("... and {} more sessions", sessions.len() - 15))
        );
    }
    println!();
}
