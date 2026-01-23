//! Cleanup command handler

use std::io::IsTerminal;

use anyhow::Result;
use std::io::{self, BufRead, Write};

use agr::storage::{SessionInfo, StorageStats};
use agr::tui::widgets::FileItem;
use agr::tui::{current_theme, CleanupApp};
use agr::{Config, StorageManager};

use super::truncate_string;

/// Interactive cleanup of old session recordings.
///
/// When stdout is a TTY, shows an interactive file explorer with multi-select.
/// When piped, shows a text-based prompt interface (fallback).
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
        let theme = current_theme();
        if agent_filter.is_some() || older_than.is_some() {
            println!(
                "{}",
                theme.primary_text("No sessions match the specified filters.")
            );
        } else {
            println!("{}", theme.primary_text("No sessions to clean up."));
        }
        return Ok(());
    }

    // Check if we're in a TTY - if so, use interactive TUI
    if std::io::stdout().is_terminal() {
        handle_tui(sessions, agent_filter, storage)
    } else {
        handle_text(sessions, agent_filter, older_than, age_threshold, storage)
    }
}

/// Handle cleanup command with interactive TUI.
fn handle_tui(
    sessions: Vec<SessionInfo>,
    agent_filter: Option<&str>,
    storage: StorageManager,
) -> Result<()> {
    // Convert sessions to FileItems
    let items: Vec<FileItem> = sessions.into_iter().map(FileItem::from).collect();

    // Create and run the cleanup app
    let mut app = CleanupApp::new(items, storage)?;

    // If agent filter was specified on command line, it's already applied
    // (sessions were filtered before being passed to this function)
    let _ = agent_filter; // Acknowledge the parameter (already used in filtering)

    app.run()
}

/// Handle cleanup command with text output (piped mode fallback).
fn handle_text(
    sessions: Vec<SessionInfo>,
    agent_filter: Option<&str>,
    older_than: Option<u32>,
    age_threshold: u32,
    storage: StorageManager,
) -> Result<()> {
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
pub(crate) fn print_header(
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
pub(crate) fn print_session_summary(total: usize, old_count: usize, age_threshold: u32) {
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
pub(crate) fn print_sessions_table(sessions: &[SessionInfo], age_threshold: u32) {
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

/// Process user input and perform deletion.
fn process_deletion_input(
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
pub(crate) fn parse_deletion_input(
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Create a mock SessionInfo for testing
    fn mock_session(agent: &str, filename: &str, age_days: i64, size: u64) -> SessionInfo {
        SessionInfo {
            path: PathBuf::from(format!("/mock/{}/{}", agent, filename)),
            agent: agent.to_string(),
            filename: filename.to_string(),
            size,
            modified: Local::now(),
            age_days,
            age_hours: age_days * 24,
            age_minutes: age_days * 24 * 60,
        }
    }

    /// Create mock StorageStats for testing
    fn mock_stats(session_count: usize, by_agent: HashMap<String, usize>) -> StorageStats {
        StorageStats {
            total_size: 1024 * session_count as u64,
            session_count,
            sessions_by_agent: by_agent,
            oldest_session: None,
            disk_percentage: 0.5,
        }
    }

    // Tests for parse_deletion_input

    #[test]
    fn parse_deletion_input_zero_returns_empty() {
        let sessions = vec![mock_session("claude", "s1.cast", 1, 100)];
        let result = parse_deletion_input("0", &sessions, 0, 14).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_deletion_input_empty_returns_empty() {
        let sessions = vec![mock_session("claude", "s1.cast", 1, 100)];
        let result = parse_deletion_input("", &sessions, 0, 14).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_deletion_input_all_returns_all() {
        let sessions = vec![
            mock_session("claude", "s1.cast", 1, 100),
            mock_session("claude", "s2.cast", 5, 200),
        ];
        let result = parse_deletion_input("all", &sessions, 0, 14).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_deletion_input_old_returns_old_sessions() {
        let sessions = vec![
            mock_session("claude", "new.cast", 1, 100),
            mock_session("claude", "old.cast", 20, 200),
        ];
        let result = parse_deletion_input("old", &sessions, 1, 14).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "old.cast");
    }

    #[test]
    fn parse_deletion_input_old_with_zero_count_returns_empty() {
        let sessions = vec![mock_session("claude", "new.cast", 1, 100)];
        let result = parse_deletion_input("old", &sessions, 0, 14).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_deletion_input_number_returns_first_n() {
        let sessions = vec![
            mock_session("claude", "s1.cast", 1, 100),
            mock_session("claude", "s2.cast", 5, 200),
            mock_session("claude", "s3.cast", 10, 300),
        ];
        let result = parse_deletion_input("2", &sessions, 0, 14).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].filename, "s1.cast");
        assert_eq!(result[1].filename, "s2.cast");
    }

    #[test]
    fn parse_deletion_input_number_exceeding_count_returns_empty() {
        let sessions = vec![mock_session("claude", "s1.cast", 1, 100)];
        let result = parse_deletion_input("5", &sessions, 0, 14).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_deletion_input_invalid_returns_empty() {
        let sessions = vec![mock_session("claude", "s1.cast", 1, 100)];
        let result = parse_deletion_input("invalid", &sessions, 0, 14).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_deletion_input_one_returns_first() {
        let sessions = vec![
            mock_session("claude", "first.cast", 1, 100),
            mock_session("claude", "second.cast", 5, 200),
        ];
        let result = parse_deletion_input("1", &sessions, 0, 14).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].filename, "first.cast");
    }

    // Tests for print_header (these are output tests, verifying no panics)

    #[test]
    fn print_header_no_filters_does_not_panic() {
        let stats = mock_stats(3, HashMap::new());
        let result = print_header(&stats, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn print_header_with_agent_filter_does_not_panic() {
        let stats = mock_stats(3, HashMap::new());
        let result = print_header(&stats, Some("claude"), None);
        assert!(result.is_ok());
    }

    #[test]
    fn print_header_with_age_filter_does_not_panic() {
        let stats = mock_stats(3, HashMap::new());
        let result = print_header(&stats, None, Some(14));
        assert!(result.is_ok());
    }

    #[test]
    fn print_header_with_both_filters_does_not_panic() {
        let stats = mock_stats(3, HashMap::new());
        let result = print_header(&stats, Some("claude"), Some(14));
        assert!(result.is_ok());
    }

    #[test]
    fn print_header_with_agents_summary_does_not_panic() {
        let mut by_agent = HashMap::new();
        by_agent.insert("claude".to_string(), 2);
        by_agent.insert("codex".to_string(), 1);
        let stats = mock_stats(3, by_agent);
        let result = print_header(&stats, None, None);
        assert!(result.is_ok());
    }

    // Tests for print_session_summary

    #[test]
    fn print_session_summary_with_old_sessions_does_not_panic() {
        // Just verifying no panics occur
        print_session_summary(10, 3, 14);
    }

    #[test]
    fn print_session_summary_with_no_old_sessions_does_not_panic() {
        print_session_summary(10, 0, 14);
    }

    #[test]
    fn print_session_summary_with_zero_sessions_does_not_panic() {
        print_session_summary(0, 0, 14);
    }

    // Tests for print_sessions_table

    #[test]
    fn print_sessions_table_empty_does_not_panic() {
        let sessions: Vec<SessionInfo> = vec![];
        print_sessions_table(&sessions, 14);
    }

    #[test]
    fn print_sessions_table_single_session_does_not_panic() {
        let sessions = vec![mock_session("claude", "test.cast", 5, 1024)];
        print_sessions_table(&sessions, 14);
    }

    #[test]
    fn print_sessions_table_with_old_sessions_does_not_panic() {
        let sessions = vec![
            mock_session("claude", "new.cast", 1, 1024),
            mock_session("claude", "old.cast", 20, 2048),
        ];
        print_sessions_table(&sessions, 14);
    }

    #[test]
    fn print_sessions_table_more_than_15_does_not_panic() {
        let sessions: Vec<SessionInfo> = (0..20)
            .map(|i| mock_session("claude", &format!("s{}.cast", i), i as i64, 1024))
            .collect();
        print_sessions_table(&sessions, 14);
    }

    #[test]
    fn print_sessions_table_with_long_agent_name_does_not_panic() {
        let sessions = vec![mock_session(
            "very-long-agent-name-that-exceeds-limit",
            "test.cast",
            5,
            1024,
        )];
        print_sessions_table(&sessions, 14);
    }
}
