//! `agr sync` command handlers, argument handling, prompts and output only.

use anyhow::Result;

use agr::cli::SyncCommands;

mod delete;
mod key;
mod migrate;
mod run;

/// Route one `agr sync ...` command.
#[cfg(not(tarpaulin_include))]
pub fn handle(cmd: SyncCommands) -> Result<()> {
    match cmd {
        SyncCommands::Status { verbose } => run::status(verbose),
        SyncCommands::Push {
            sessions,
            dry_run,
            yes,
            overwrite,
            auto,
            verbose,
        } => run::push(
            run::Args {
                sessions,
                dry_run,
                yes,
                overwrite,
                verbose,
            },
            auto,
        ),
        SyncCommands::Pull {
            sessions,
            dry_run,
            yes,
            overwrite,
            verbose,
        } => run::pull(run::Args {
            sessions,
            dry_run,
            yes,
            overwrite,
            verbose,
        }),
        SyncCommands::Delete {
            sessions,
            all,
            force,
            dry_run,
            yes,
            verbose,
        } => delete::delete(
            &delete::Args {
                patterns: sessions,
                all,
                force,
                dry_run,
                yes,
                verbose,
            },
            super::prompt::stdin_is_tty(),
        ),
        SyncCommands::Migrate { yes } => migrate::migrate(yes, super::prompt::stdin_is_tty()),
        SyncCommands::Key(cmd) => key::handle(cmd),
    }
}
