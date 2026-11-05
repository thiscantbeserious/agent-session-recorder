//! Maps parsed CLI commands to their handler in the `commands` module.

use anyhow::Result;

use crate::commands;
use agr::cli::{
    AgentCommands, Cli, Commands, ConfigCommands, MarkerCommands, NoWrapCommands, ShellCommands,
};

/// Dispatch the parsed command to its handler.
#[cfg(not(tarpaulin_include))]
pub fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Record { agent, name, args } => {
            commands::record::handle(&agent, name.as_deref(), &args)
        }
        Commands::Status => commands::status::handle(),
        Commands::Cleanup { agent, older_than } => {
            commands::cleanup::handle(agent.as_deref(), older_than)
        }
        Commands::List { agent } => commands::list::handle(agent.as_deref()),
        Commands::Analyze {
            file,
            agent,
            workers,
            timeout,
            no_parallel,
            curate,
            debug,
            output,
            fast,
            wait,
        } => commands::analyze::handle(commands::analyze::AnalyzeArgs {
            file,
            agent,
            workers,
            timeout,
            no_parallel,
            curate,
            debug,
            output,
            fast,
            wait,
        }),
        Commands::Play { file } => commands::play::handle(&file),
        Commands::Copy { file } => commands::copy::handle(&file),
        Commands::Marker(cmd) => match cmd {
            MarkerCommands::Add { file, time, label } => {
                commands::marker::handle_add(&file, time, &label)
            }
            MarkerCommands::List { file } => commands::marker::handle_list(&file),
        },
        Commands::Agents(cmd) => match cmd {
            AgentCommands::List => commands::agents::handle_list(),
            AgentCommands::Add { name } => commands::agents::handle_add(&name),
            AgentCommands::Remove { name } => commands::agents::handle_remove(&name),
            AgentCommands::IsWrapped { name } => commands::agents::handle_is_wrapped(&name),
            AgentCommands::NoWrap(nowrap_cmd) => match nowrap_cmd {
                NoWrapCommands::List => commands::agents::handle_nowrap_list(),
                NoWrapCommands::Add { name } => commands::agents::handle_nowrap_add(&name),
                NoWrapCommands::Remove { name } => commands::agents::handle_nowrap_remove(&name),
            },
        },
        Commands::Config(cmd) => match cmd {
            ConfigCommands::Show => commands::config::handle_show(),
            ConfigCommands::Edit => commands::config::handle_edit(),
            ConfigCommands::Migrate { yes } => commands::config::handle_migrate(yes),
            ConfigCommands::Reset { yes } => commands::config::handle_reset(yes),
        },
        Commands::Shell(cmd) => match cmd {
            ShellCommands::Status => commands::shell::handle_status(),
            ShellCommands::Install => commands::shell::handle_install(),
            ShellCommands::Uninstall => commands::shell::handle_uninstall(),
        },
        Commands::Optimize {
            remove_silence,
            output,
            file,
        } => {
            // Parse the threshold from the optional string value
            let threshold = match remove_silence {
                Some(ref s) if !s.is_empty() => {
                    let parsed: f64 = s.parse().map_err(|_| {
                        anyhow::anyhow!("Invalid threshold '{}': must be a positive number", s)
                    })?;
                    Some(parsed)
                }
                _ => None, // No value provided, will use header or default
            };

            // Currently only silence removal is supported
            if remove_silence.is_none() {
                anyhow::bail!("No optimization specified. Use --remove-silence to remove silence.");
            }

            commands::transform::handle_remove_silence(&file, threshold, output.as_deref())
        }
        Commands::Completions {
            shell,
            shell_init,
            debug,
            files,
            limit,
            prefix,
        } => commands::completions::handle::<Cli>(shell, shell_init, debug, files, limit, &prefix),
    }
}
