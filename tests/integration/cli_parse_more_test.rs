//! CLI argument parsing tests: completions limit, list, marker, record, agents, config, status, play, copy.

use agr::cli::{AgentCommands, Cli, Commands, ConfigCommands, MarkerCommands};
use clap::{CommandFactory, Parser};

#[test]
fn cli_completions_limit_defaults_to_10() {
    let cli = Cli::try_parse_from(["agr", "completions", "--files"]).unwrap();
    match cli.command {
        Commands::Completions { limit, .. } => {
            assert_eq!(limit, 10);
        }
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn cli_completions_is_hidden() {
    // The completions command should not appear in --help output
    let cmd = Cli::command();
    let subcommands: Vec<_> = cmd.get_subcommands().collect();
    let completions_cmd = subcommands.iter().find(|c| c.get_name() == "completions");
    assert!(
        completions_cmd.is_some(),
        "Completions command should exist"
    );
    assert!(
        completions_cmd.unwrap().is_hide_set(),
        "Completions command should be hidden"
    );
}

#[test]
fn cli_list_has_ls_alias() {
    // Test that 'ls' is accepted as an alias for 'list'
    let cli = Cli::try_parse_from(["agr", "ls"]).unwrap();
    match cli.command {
        Commands::List { agent } => {
            assert!(agent.is_none());
        }
        _ => panic!("Expected List command from 'ls' alias"),
    }
}

#[test]
fn cli_ls_alias_accepts_agent_argument() {
    let cli = Cli::try_parse_from(["agr", "ls", "claude"]).unwrap();
    match cli.command {
        Commands::List { agent } => {
            assert_eq!(agent, Some("claude".to_string()));
        }
        _ => panic!("Expected List command from 'ls' alias with agent"),
    }
}

#[test]
fn cli_list_alias_is_visible() {
    // The 'ls' alias should be visible in help
    let cmd = Cli::command();
    let subcommands: Vec<_> = cmd.get_subcommands().collect();
    let list_cmd = subcommands.iter().find(|c| c.get_name() == "list");
    assert!(list_cmd.is_some(), "List command should exist");

    // Check that visible_alias is set
    let aliases: Vec<_> = list_cmd.unwrap().get_visible_aliases().collect();
    assert!(
        aliases.contains(&"ls"),
        "List command should have 'ls' as visible alias"
    );
}

#[test]
fn cli_marker_add_parses() {
    let cli =
        Cli::try_parse_from(["agr", "marker", "add", "test.cast", "45.2", "marker label"]).unwrap();
    match cli.command {
        Commands::Marker(MarkerCommands::Add { file, time, label }) => {
            assert_eq!(file, "test.cast");
            assert!((time - 45.2).abs() < f64::EPSILON);
            assert_eq!(label, "marker label");
        }
        _ => panic!("Expected Marker Add command"),
    }
}

#[test]
fn cli_marker_list_parses() {
    let cli = Cli::try_parse_from(["agr", "marker", "list", "test.cast"]).unwrap();
    match cli.command {
        Commands::Marker(MarkerCommands::List { file }) => {
            assert_eq!(file, "test.cast");
        }
        _ => panic!("Expected Marker List command"),
    }
}

#[test]
fn cli_record_parses_with_agent_only() {
    let cli = Cli::try_parse_from(["agr", "record", "claude"]).unwrap();
    match cli.command {
        Commands::Record { agent, name, args } => {
            assert_eq!(agent, "claude");
            assert!(name.is_none());
            assert!(args.is_empty());
        }
        _ => panic!("Expected Record command"),
    }
}

#[test]
fn cli_record_parses_with_name() {
    let cli = Cli::try_parse_from(["agr", "record", "claude", "--name", "my-session"]).unwrap();
    match cli.command {
        Commands::Record { agent, name, args } => {
            assert_eq!(agent, "claude");
            assert_eq!(name, Some("my-session".to_string()));
            assert!(args.is_empty());
        }
        _ => panic!("Expected Record command"),
    }
}

#[test]
fn cli_record_parses_with_passthrough_args() {
    let cli = Cli::try_parse_from(["agr", "record", "claude", "--", "--help", "some-arg"]).unwrap();
    match cli.command {
        Commands::Record { agent, name, args } => {
            assert_eq!(agent, "claude");
            assert!(name.is_none());
            assert_eq!(args, vec!["--help", "some-arg"]);
        }
        _ => panic!("Expected Record command"),
    }
}

#[test]
fn cli_agents_list_parses() {
    let cli = Cli::try_parse_from(["agr", "agents", "list"]).unwrap();
    match cli.command {
        Commands::Agents(AgentCommands::List) => {}
        _ => panic!("Expected Agents List command"),
    }
}

#[test]
fn cli_agents_add_parses() {
    let cli = Cli::try_parse_from(["agr", "agents", "add", "my-agent"]).unwrap();
    match cli.command {
        Commands::Agents(AgentCommands::Add { name }) => {
            assert_eq!(name, "my-agent");
        }
        _ => panic!("Expected Agents Add command"),
    }
}

#[test]
fn cli_agents_remove_parses() {
    let cli = Cli::try_parse_from(["agr", "agents", "remove", "old-agent"]).unwrap();
    match cli.command {
        Commands::Agents(AgentCommands::Remove { name }) => {
            assert_eq!(name, "old-agent");
        }
        _ => panic!("Expected Agents Remove command"),
    }
}

#[test]
fn cli_config_show_parses() {
    let cli = Cli::try_parse_from(["agr", "config", "show"]).unwrap();
    match cli.command {
        Commands::Config(ConfigCommands::Show) => {}
        _ => panic!("Expected Config Show command"),
    }
}

#[test]
fn cli_config_edit_parses() {
    let cli = Cli::try_parse_from(["agr", "config", "edit"]).unwrap();
    match cli.command {
        Commands::Config(ConfigCommands::Edit) => {}
        _ => panic!("Expected Config Edit command"),
    }
}

#[test]
fn cli_config_migrate_parses() {
    let cli = Cli::try_parse_from(["agr", "config", "migrate"]).unwrap();
    match cli.command {
        Commands::Config(ConfigCommands::Migrate { yes }) => {
            assert!(!yes);
        }
        _ => panic!("Expected Config Migrate command"),
    }
}

#[test]
fn cli_config_migrate_parses_with_yes_flag() {
    let cli = Cli::try_parse_from(["agr", "config", "migrate", "--yes"]).unwrap();
    match cli.command {
        Commands::Config(ConfigCommands::Migrate { yes }) => {
            assert!(yes);
        }
        _ => panic!("Expected Config Migrate command"),
    }
}

#[test]
fn cli_config_migrate_parses_with_short_yes_flag() {
    let cli = Cli::try_parse_from(["agr", "config", "migrate", "-y"]).unwrap();
    match cli.command {
        Commands::Config(ConfigCommands::Migrate { yes }) => {
            assert!(yes);
        }
        _ => panic!("Expected Config Migrate command"),
    }
}

#[test]
fn cli_config_reset_parses() {
    let cli = Cli::try_parse_from(["agr", "config", "reset"]).unwrap();
    match cli.command {
        Commands::Config(ConfigCommands::Reset { yes }) => {
            assert!(!yes);
        }
        _ => panic!("Expected Config Reset command"),
    }
}

#[test]
fn cli_config_reset_parses_with_yes_flag() {
    let cli = Cli::try_parse_from(["agr", "config", "reset", "--yes"]).unwrap();
    match cli.command {
        Commands::Config(ConfigCommands::Reset { yes }) => {
            assert!(yes);
        }
        _ => panic!("Expected Config Reset command"),
    }
}

#[test]
fn cli_status_parses() {
    let cli = Cli::try_parse_from(["agr", "status"]).unwrap();
    match cli.command {
        Commands::Status => {}
        _ => panic!("Expected Status command"),
    }
}

#[test]
fn cli_play_parses_with_file() {
    let cli = Cli::try_parse_from(["agr", "play", "session.cast"]).unwrap();
    match cli.command {
        Commands::Play { file } => {
            assert_eq!(file, "session.cast");
        }
        _ => panic!("Expected Play command"),
    }
}

#[test]
fn cli_play_parses_with_path() {
    let cli = Cli::try_parse_from(["agr", "play", "/path/to/session.cast"]).unwrap();
    match cli.command {
        Commands::Play { file } => {
            assert_eq!(file, "/path/to/session.cast");
        }
        _ => panic!("Expected Play command"),
    }
}

#[test]
fn cli_play_parses_with_short_format() {
    let cli = Cli::try_parse_from(["agr", "play", "claude/session.cast"]).unwrap();
    match cli.command {
        Commands::Play { file } => {
            assert_eq!(file, "claude/session.cast");
        }
        _ => panic!("Expected Play command"),
    }
}

#[test]
fn cli_copy_parses_with_file() {
    let cli = Cli::try_parse_from(["agr", "copy", "session.cast"]).unwrap();
    match cli.command {
        Commands::Copy { file } => {
            assert_eq!(file, "session.cast");
        }
        _ => panic!("Expected Copy command"),
    }
}

#[test]
fn cli_copy_parses_with_path() {
    let cli = Cli::try_parse_from(["agr", "copy", "/path/to/session.cast"]).unwrap();
    match cli.command {
        Commands::Copy { file } => {
            assert_eq!(file, "/path/to/session.cast");
        }
        _ => panic!("Expected Copy command"),
    }
}

#[test]
fn cli_copy_parses_with_short_format() {
    let cli = Cli::try_parse_from(["agr", "copy", "claude/session.cast"]).unwrap();
    match cli.command {
        Commands::Copy { file } => {
            assert_eq!(file, "claude/session.cast");
        }
        _ => panic!("Expected Copy command"),
    }
}

#[test]
fn cli_copy_requires_file_argument() {
    // `agr copy` without file should fail
    let result = Cli::try_parse_from(["agr", "copy"]);
    assert!(result.is_err());
}
