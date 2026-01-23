//! List command handler

use anyhow::Result;

use agr::tui::current_theme;
use agr::{Config, StorageManager};

use super::truncate_string;

/// List all recorded sessions with details.
///
/// Shows sessions sorted by date (newest first) with agent name,
/// age, file size, and filename.
#[cfg(not(tarpaulin_include))]
pub fn handle(agent: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    let storage = StorageManager::new(config);
    let mut sessions = storage.list_sessions(agent)?;
    let theme = current_theme();

    if sessions.is_empty() {
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
