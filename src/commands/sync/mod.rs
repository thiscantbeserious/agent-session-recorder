//! `agr sync` command handlers, argument handling, prompts and output only.

use anyhow::Result;

use agr::cli::SyncCommands;

mod key;
mod run;

/// Route one `agr sync ...` command.
#[cfg(not(tarpaulin_include))]
pub fn handle(cmd: SyncCommands) -> Result<()> {
    match cmd {
        SyncCommands::Status => run::status(),
        SyncCommands::Push {
            sessions,
            dry_run,
            yes,
            overwrite,
            auto,
        } => run::push(
            run::Args {
                sessions,
                dry_run,
                yes,
                overwrite,
            },
            auto,
        ),
        SyncCommands::Pull {
            sessions,
            dry_run,
            yes,
            overwrite,
        } => run::pull(run::Args {
            sessions,
            dry_run,
            yes,
            overwrite,
        }),
        SyncCommands::Delete {
            sessions,
            dry_run,
            yes,
        } => run::delete(run::Args {
            sessions,
            dry_run,
            yes,
            overwrite: false,
        }),
        SyncCommands::Key(cmd) => key::handle(cmd),
    }
}
