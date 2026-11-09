//! Clap parsing of `agr sync ...`

use agr::cli::{Cli, Commands, SyncCommands, SyncKeyCommands};
use clap::Parser;

fn sync(args: &[&str]) -> SyncCommands {
    let mut argv = vec!["agr", "sync"];
    argv.extend_from_slice(args);
    match Cli::try_parse_from(argv).unwrap().command {
        Commands::Sync(cmd) => cmd,
        _ => panic!("Expected Sync command"),
    }
}

#[test]
fn status_parses() {
    assert!(matches!(
        sync(&["status"]),
        SyncCommands::Status { verbose: false }
    ));
}

#[test]
fn status_verbose_parses_long_and_short() {
    for flag in ["-v", "--verbose"] {
        assert!(matches!(
            sync(&["status", flag]),
            SyncCommands::Status { verbose: true }
        ));
    }
}

#[test]
fn push_defaults_and_flags() {
    let SyncCommands::Push {
        sessions,
        dry_run,
        yes,
        overwrite,
        auto,
    } = sync(&["push"])
    else {
        panic!("Expected Push");
    };
    assert_eq!(
        (dry_run, yes, overwrite, auto),
        (false, false, false, false)
    );
    assert!(sessions.is_empty());
    let SyncCommands::Push {
        sessions,
        dry_run,
        yes,
        overwrite,
        auto,
    } = sync(&[
        "push",
        "--dry-run",
        "-y",
        "--overwrite",
        "--auto",
        "a/x.cast.gz",
        "b/y",
    ])
    else {
        panic!("Expected Push");
    };
    assert_eq!((dry_run, yes, overwrite, auto), (true, true, true, true));
    assert_eq!(sessions, ["a/x.cast.gz", "b/y"]);
}

#[test]
fn pull_flags_and_sessions() {
    let SyncCommands::Pull {
        sessions,
        dry_run,
        yes,
        overwrite,
    } = sync(&["pull", "--yes", "--overwrite", "a/x"])
    else {
        panic!("Expected Pull");
    };
    assert_eq!((dry_run, yes, overwrite), (false, true, true));
    assert_eq!(sessions, ["a/x"]);
}

#[test]
fn delete_needs_a_session() {
    assert!(Cli::try_parse_from(["agr", "sync", "delete"]).is_err());
    let SyncCommands::Delete {
        sessions,
        dry_run,
        yes,
    } = sync(&["delete", "--dry-run", "a/x"])
    else {
        panic!("Expected Delete");
    };
    assert_eq!((dry_run, yes), (true, false));
    assert_eq!(sessions, ["a/x"]);
}

#[test]
fn key_export_and_import() {
    assert!(matches!(
        sync(&["key", "export"]),
        SyncCommands::Key(SyncKeyCommands::Export)
    ));
    let SyncCommands::Key(SyncKeyCommands::Import { phrase, force }) =
        sync(&["key", "import", "--force", "agr1-AAAA"])
    else {
        panic!("Expected Import");
    };
    assert_eq!((phrase.as_deref(), force), (Some("agr1-AAAA"), true));
    let SyncCommands::Key(SyncKeyCommands::Import { phrase, force }) = sync(&["key", "import"])
    else {
        panic!("Expected Import");
    };
    assert_eq!((phrase, force), (None, false));
}

#[test]
fn auto_flag_is_hidden_from_help() {
    use clap::CommandFactory;
    let cmd = Cli::command();
    let push = cmd
        .find_subcommand("sync")
        .and_then(|s| s.find_subcommand("push"))
        .unwrap();
    let auto = push.get_arguments().find(|a| a.get_id() == "auto").unwrap();
    assert!(auto.is_hide_set());
}
