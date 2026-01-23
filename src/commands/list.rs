//! List command handler

use std::io::IsTerminal;

use anyhow::Result;

use agr::tui::widgets::FileItem;
use agr::tui::{current_theme, ListApp};
use agr::{Config, StorageManager};

use super::truncate_string;

/// List all recorded sessions with details.
///
/// When stdout is a TTY, shows an interactive file explorer.
/// When piped, shows a simple text table (fallback).
#[cfg(not(tarpaulin_include))]
pub fn handle(agent: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let storage = StorageManager::new(config);
    let sessions = storage.list_sessions(agent)?;

    if sessions.is_empty() {
        let theme = current_theme();
        if let Some(agent_name) = agent {
            println!(
                "{}",
                theme.primary_text(&format!("No sessions found for agent '{}'.", agent_name))
            );
        } else {
            println!("{}", theme.primary_text("No sessions found."));
        }
        return Ok(());
    }

    // Check if we're in a TTY - if so, use interactive TUI
    if std::io::stdout().is_terminal() {
        handle_tui(sessions, agent)
    } else {
        handle_text(sessions, agent, &storage)
    }
}

/// Handle list command with interactive TUI.
fn handle_tui(sessions: Vec<agr::storage::SessionInfo>, agent: Option<&str>) -> Result<()> {
    // Convert sessions to FileItems
    let items: Vec<FileItem> = sessions.into_iter().map(FileItem::from).collect();

    // Create and run the list app
    let mut app = ListApp::new(items)?;

    // If agent filter was specified on command line, apply it
    if let Some(agent_name) = agent {
        app.set_agent_filter(agent_name);
    }

    app.run()
}

/// Handle list command with text output (piped mode fallback).
fn handle_text(
    mut sessions: Vec<agr::storage::SessionInfo>,
    agent: Option<&str>,
    storage: &StorageManager,
) -> Result<()> {
    let theme = current_theme();

    // Reverse to show newest first
    sessions.reverse();

    // Print summary header
    if let Some(agent_name) = &agent {
        // Just show count for filtered view
        println!(
            "{}",
            theme.primary_text(&format!(
                "Sessions: {} (filtered by agent: {})",
                sessions.len(),
                agent_name
            ))
        );
    } else {
        // Show full summary with agent breakdown
        let stats = storage.get_stats()?;
        let mut agents: Vec<_> = stats.sessions_by_agent.iter().collect();
        agents.sort_by(|a, b| a.0.cmp(b.0));
        let agents_summary: Vec<String> = agents
            .iter()
            .map(|(agent, count)| format!("{}: {}", agent, count))
            .collect();
        if agents_summary.is_empty() {
            println!(
                "{}",
                theme.primary_text(&format!("Sessions: {} total", stats.session_count))
            );
        } else {
            println!(
                "{}",
                theme.primary_text(&format!(
                    "Sessions: {} total ({})",
                    stats.session_count,
                    agents_summary.join(", ")
                ))
            );
        }
    }
    println!();

    // Print table header
    println!(
        "{}",
        theme.primary_text("  #  |  Age  | DateTime         | Agent       | Size       | Filename")
    );
    println!(
        "{}",
        theme.primary_text(
            "-----+-------+------------------+-------------+------------+---------------------------"
        )
    );

    // Display sessions in formatted table
    for (i, session) in sessions.iter().enumerate() {
        println!(
            "{}",
            theme.primary_text(&format!(
                "{:>3}  | {:>5} | {} | {:11} | {:>10} | {}",
                i + 1,
                session.format_age(),
                session.modified.format("%Y-%m-%d %H:%M"),
                truncate_string(&session.agent, 11),
                session.size_human(),
                session.filename
            ))
        );
    }

    Ok(())
}
