//! Cleanup command handler

use std::io::IsTerminal;

use anyhow::Result;

use agr::storage::SessionInfo;
use agr::theme::current_theme;
use agr::tui::app::TuiApp;
use agr::tui::widgets::FileItem;
use agr::tui::CleanupApp;
use agr::{Config, StorageManager};

mod delete;
pub mod orphans;
mod print;

#[cfg(test)]
use delete::parse_deletion_input;
use delete::process_deletion_input;
use print::{print_header, print_session_summary, print_sessions_table};

/// Interactive cleanup of old session recordings.
///
/// First offers to remove orphaned backups and stale locks (`yes` skips that question).
///
/// When stdout is a TTY, shows an interactive file explorer with multi-select.
/// When piped, shows a text-based prompt interface (fallback).
/// Supports filtering by agent and age threshold.
#[cfg(not(tarpaulin_include))]
pub fn handle(agent_filter: Option<&str>, older_than: Option<u32>, yes: bool) -> Result<()> {
    let config = Config::load()?;
    let age_threshold = config.storage.age_threshold_days;
    let storage = StorageManager::new(config.clone());
    orphans::handle(&storage.storage_dir(), yes)?;

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
        handle_tui(sessions, agent_filter, config)
    } else {
        handle_text(sessions, agent_filter, older_than, age_threshold, storage)
    }
}

/// Handle cleanup command with interactive TUI.
fn handle_tui(
    sessions: Vec<SessionInfo>,
    agent_filter: Option<&str>,
    config: Config,
) -> Result<()> {
    // Convert sessions to FileItems
    let items: Vec<FileItem> = sessions.into_iter().map(FileItem::from).collect();

    // Create and run the cleanup app
    let mut app = CleanupApp::new(items, config)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use agr::storage::StorageStats;
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
            uncompressed_total: agr::files::codec::UncompressedSize::Exact(0),
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
