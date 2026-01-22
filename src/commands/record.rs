//! Record command handler

use anyhow::Result;

use agr::{Config, Recorder};

/// Start recording an AI agent session.
///
/// Creates a new recording in ~/recorded_agent_sessions/<agent>/<timestamp>.cast.
/// Warns if the agent is not in the configured list.
#[cfg(not(tarpaulin_include))]
pub fn handle(agent: &str, name: Option<&str>, args: &[String]) -> Result<()> {
    let config = Config::load()?;

    if !config.is_agent_enabled(agent) {
        eprintln!("Warning: Agent '{}' is not in the configured list.", agent);
        eprintln!("Add it with: agr agents add {}", agent);
        eprintln!();
    }

    let mut recorder = Recorder::new(config);
    recorder.record(agent, name, args)
}
