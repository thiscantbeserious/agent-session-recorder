//! Agents subcommands handler

use anyhow::Result;

use agr::tui::current_theme;
use agr::Config;

/// List all configured agents.
#[cfg(not(tarpaulin_include))]
pub fn handle_list() -> Result<()> {
    let config = Config::load()?;
    let theme = current_theme();

    if config.agents.enabled.is_empty() {
        println!("{}", theme.primary_text("No agents configured."));
        return Ok(());
    }

    println!("{}", theme.primary_text("Configured agents:"));
    for agent in &config.agents.enabled {
        println!("{}", theme.primary_text(&format!("  {}", agent)));
    }

    Ok(())
}

/// Add an agent to the configuration.
#[cfg(not(tarpaulin_include))]
pub fn handle_add(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    let theme = current_theme();

    if config.add_agent(name) {
        config.save()?;
        println!("{}", theme.primary_text(&format!("Added agent: {}", name)));
    } else {
        println!(
            "{}",
            theme.primary_text(&format!("Agent '{}' is already configured.", name))
        );
    }

    Ok(())
}

/// Remove an agent from the configuration.
#[cfg(not(tarpaulin_include))]
pub fn handle_remove(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    let theme = current_theme();

    if config.remove_agent(name) {
        config.save()?;
        println!(
            "{}",
            theme.primary_text(&format!("Removed agent: {}", name))
        );
    } else {
        println!(
            "{}",
            theme.primary_text(&format!("Agent '{}' was not configured.", name))
        );
    }

    Ok(())
}

/// Check if an agent should be wrapped by shell integration.
///
/// Exits with code 0 if should wrap, 1 if not.
#[cfg(not(tarpaulin_include))]
pub fn handle_is_wrapped(name: &str) -> Result<()> {
    let config = Config::load()?;

    if config.should_wrap_agent(name) {
        // Exit code 0 = should wrap
        std::process::exit(0);
    } else {
        // Exit code 1 = should not wrap
        std::process::exit(1);
    }
}

/// List agents that are excluded from auto-wrapping.
#[cfg(not(tarpaulin_include))]
pub fn handle_nowrap_list() -> Result<()> {
    let config = Config::load()?;
    let theme = current_theme();

    if config.agents.no_wrap.is_empty() {
        println!(
            "{}",
            theme.primary_text(
                "No agents in no-wrap list. All enabled agents will be auto-wrapped."
            )
        );
    } else {
        println!("{}", theme.primary_text("Agents not auto-wrapped:"));
        for agent in &config.agents.no_wrap {
            println!("{}", theme.primary_text(&format!("  {}", agent)));
        }
    }

    Ok(())
}

/// Add an agent to the no-wrap list.
#[cfg(not(tarpaulin_include))]
pub fn handle_nowrap_add(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    let theme = current_theme();

    if config.add_no_wrap(name) {
        config.save()?;
        println!(
            "{}",
            theme.primary_text(&format!(
                "Added '{}' to no-wrap list. It will not be auto-wrapped.",
                name
            ))
        );
    } else {
        println!(
            "{}",
            theme.primary_text(&format!("Agent '{}' is already in the no-wrap list.", name))
        );
    }

    Ok(())
}

/// Remove an agent from the no-wrap list.
#[cfg(not(tarpaulin_include))]
pub fn handle_nowrap_remove(name: &str) -> Result<()> {
    let mut config = Config::load()?;
    let theme = current_theme();

    if config.remove_no_wrap(name) {
        config.save()?;
        println!(
            "{}",
            theme.primary_text(&format!(
                "Removed '{}' from no-wrap list. It will now be auto-wrapped.",
                name
            ))
        );
    } else {
        println!(
            "{}",
            theme.primary_text(&format!("Agent '{}' was not in the no-wrap list.", name))
        );
    }

    Ok(())
}
