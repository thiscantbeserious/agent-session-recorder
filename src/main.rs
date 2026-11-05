//! Agent Session Recorder (AGR) - CLI entry point
//!
//! This file handles argument parsing and command dispatch.
//! CLI struct definitions are in the `cli` module (src/cli.rs).
//! Command implementations are in the `commands` module, routing is in `dispatch`.

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};

mod commands;
mod dispatch;
mod help;

use agr::cli::{build_cli_styles, Cli};

#[cfg(not(tarpaulin_include))]
fn main() -> Result<()> {
    // Check for interactive TUI help
    if help::should_show_tui_help() {
        return help::show_tui_help();
    }

    // Build command with styles, logo, and custom version
    let cmd = Cli::command()
        .styles(build_cli_styles())
        .before_help(help::build_logo())
        .version(help::build_version());

    // Try to parse, handling help requests with themed output
    let matches = help::parse_matches(cmd);

    let cli = Cli::from_arg_matches(&matches).unwrap();

    dispatch::dispatch(cli)
}
