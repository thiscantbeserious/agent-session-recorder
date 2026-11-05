//! CLI argument parsing tests: cleanup, shell, analyze, completions.

use agr::cli::{Cli, Commands, ShellCommands};
use clap::Parser;
use clap_complete::Shell as CompletionShell;

#[test]
fn cli_cleanup_parses_with_no_args() {
    let cli = Cli::try_parse_from(["agr", "cleanup"]).unwrap();
    match cli.command {
        Commands::Cleanup { agent, older_than } => {
            assert!(agent.is_none());
            assert!(older_than.is_none());
        }
        _ => panic!("Expected Cleanup command"),
    }
}

#[test]
fn cli_cleanup_parses_with_agent_flag() {
    let cli = Cli::try_parse_from(["agr", "cleanup", "--agent", "claude"]).unwrap();
    match cli.command {
        Commands::Cleanup { agent, older_than } => {
            assert_eq!(agent, Some("claude".to_string()));
            assert!(older_than.is_none());
        }
        _ => panic!("Expected Cleanup command"),
    }
}

#[test]
fn cli_cleanup_parses_with_older_than_flag() {
    let cli = Cli::try_parse_from(["agr", "cleanup", "--older-than", "30"]).unwrap();
    match cli.command {
        Commands::Cleanup { agent, older_than } => {
            assert!(agent.is_none());
            assert_eq!(older_than, Some(30));
        }
        _ => panic!("Expected Cleanup command"),
    }
}

#[test]
fn cli_cleanup_parses_with_both_flags() {
    let cli =
        Cli::try_parse_from(["agr", "cleanup", "--agent", "codex", "--older-than", "60"]).unwrap();
    match cli.command {
        Commands::Cleanup { agent, older_than } => {
            assert_eq!(agent, Some("codex".to_string()));
            assert_eq!(older_than, Some(60));
        }
        _ => panic!("Expected Cleanup command"),
    }
}

#[test]
fn cli_shell_status_parses() {
    let cli = Cli::try_parse_from(["agr", "shell", "status"]).unwrap();
    match cli.command {
        Commands::Shell(ShellCommands::Status) => {}
        _ => panic!("Expected Shell Status command"),
    }
}

#[test]
fn cli_shell_install_parses() {
    let cli = Cli::try_parse_from(["agr", "shell", "install"]).unwrap();
    match cli.command {
        Commands::Shell(ShellCommands::Install) => {}
        _ => panic!("Expected Shell Install command"),
    }
}

#[test]
fn cli_shell_uninstall_parses() {
    let cli = Cli::try_parse_from(["agr", "shell", "uninstall"]).unwrap();
    match cli.command {
        Commands::Shell(ShellCommands::Uninstall) => {}
        _ => panic!("Expected Shell Uninstall command"),
    }
}

#[test]
fn cli_analyze_parses_with_file_only() {
    let cli = Cli::try_parse_from(["agr", "analyze", "session.cast"]).unwrap();
    match cli.command {
        Commands::Analyze {
            file,
            agent,
            workers,
            timeout,
            no_parallel,
            curate,
            debug: _,
            output: _,
            fast,
            wait,
        } => {
            assert_eq!(file, "session.cast");
            assert!(agent.is_none());
            assert!(workers.is_none());
            assert!(timeout.is_none());
            assert!(!no_parallel);
            assert!(!curate);
            assert!(!fast);
            assert!(!wait);
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_analyze_parses_with_agent_flag() {
    let cli = Cli::try_parse_from(["agr", "analyze", "session.cast", "--agent", "codex"]).unwrap();
    match cli.command {
        Commands::Analyze { file, agent, .. } => {
            assert_eq!(file, "session.cast");
            assert_eq!(agent, Some("codex".to_string()));
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_analyze_parses_with_short_agent_flag() {
    let cli = Cli::try_parse_from(["agr", "analyze", "session.cast", "-a", "claude"]).unwrap();
    match cli.command {
        Commands::Analyze { file, agent, .. } => {
            assert_eq!(file, "session.cast");
            assert_eq!(agent, Some("claude".to_string()));
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_analyze_parses_with_path() {
    let cli = Cli::try_parse_from(["agr", "analyze", "/path/to/session.cast"]).unwrap();
    match cli.command {
        Commands::Analyze { file, agent, .. } => {
            assert_eq!(file, "/path/to/session.cast");
            assert!(agent.is_none());
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_analyze_parses_with_workers_flag() {
    let cli = Cli::try_parse_from(["agr", "analyze", "session.cast", "--workers", "4"]).unwrap();
    match cli.command {
        Commands::Analyze { file, workers, .. } => {
            assert_eq!(file, "session.cast");
            assert_eq!(workers, Some(4));
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_analyze_parses_with_timeout_flag() {
    let cli = Cli::try_parse_from(["agr", "analyze", "session.cast", "--timeout", "180"]).unwrap();
    match cli.command {
        Commands::Analyze { file, timeout, .. } => {
            assert_eq!(file, "session.cast");
            assert_eq!(timeout, Some(180));
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_analyze_parses_with_no_parallel_flag() {
    let cli = Cli::try_parse_from(["agr", "analyze", "session.cast", "--no-parallel"]).unwrap();
    match cli.command {
        Commands::Analyze {
            file, no_parallel, ..
        } => {
            assert_eq!(file, "session.cast");
            assert!(no_parallel);
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_analyze_parses_with_all_flags() {
    let cli = Cli::try_parse_from([
        "agr",
        "analyze",
        "session.cast",
        "--agent",
        "codex",
        "--workers",
        "2",
        "--timeout",
        "60",
        "--no-parallel",
        "--curate",
        "--debug",
        "--output=debug.txt",
    ])
    .unwrap();
    match cli.command {
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
        } => {
            assert_eq!(file, "session.cast");
            assert_eq!(agent, Some("codex".to_string()));
            assert_eq!(workers, Some(2));
            assert_eq!(timeout, Some(60));
            assert!(no_parallel);
            assert!(curate);
            assert!(debug);
            assert_eq!(output, Some("debug.txt".to_string()));
            assert!(!fast);
            assert!(!wait);
        }
        _ => panic!("Expected Analyze command"),
    }
}

#[test]
fn cli_completions_parses_with_shell_flag() {
    let cli = Cli::try_parse_from(["agr", "completions", "--shell", "bash"]).unwrap();
    match cli.command {
        Commands::Completions {
            shell,
            files,
            prefix,
            ..
        } => {
            assert_eq!(shell, Some(CompletionShell::Bash));
            assert!(!files);
            assert_eq!(prefix, "");
        }
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn cli_completions_parses_with_files_flag() {
    let cli = Cli::try_parse_from(["agr", "completions", "--files"]).unwrap();
    match cli.command {
        Commands::Completions {
            shell,
            files,
            prefix,
            ..
        } => {
            assert!(shell.is_none());
            assert!(files);
            assert_eq!(prefix, "");
        }
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn cli_completions_parses_with_files_and_prefix() {
    let cli = Cli::try_parse_from(["agr", "completions", "--files", "claude/"]).unwrap();
    match cli.command {
        Commands::Completions {
            shell,
            files,
            prefix,
            ..
        } => {
            assert!(shell.is_none());
            assert!(files);
            assert_eq!(prefix, "claude/");
        }
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn cli_completions_parses_with_shell_init_zsh() {
    let cli = Cli::try_parse_from(["agr", "completions", "--shell-init", "zsh"]).unwrap();
    match cli.command {
        Commands::Completions {
            shell,
            shell_init,
            debug,
            files,
            limit,
            prefix,
        } => {
            assert!(shell.is_none());
            assert_eq!(shell_init, Some(CompletionShell::Zsh));
            assert!(!debug);
            assert!(!files);
            assert_eq!(limit, 10);
            assert_eq!(prefix, "");
        }
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn cli_completions_parses_with_shell_init_bash() {
    let cli = Cli::try_parse_from(["agr", "completions", "--shell-init", "bash"]).unwrap();
    match cli.command {
        Commands::Completions { shell_init, .. } => {
            assert_eq!(shell_init, Some(CompletionShell::Bash));
        }
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn cli_completions_parses_with_limit() {
    let cli = Cli::try_parse_from(["agr", "completions", "--files", "--limit", "20"]).unwrap();
    match cli.command {
        Commands::Completions { files, limit, .. } => {
            assert!(files);
            assert_eq!(limit, 20);
        }
        _ => panic!("Expected Completions command"),
    }
}
