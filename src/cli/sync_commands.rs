//! The `agr sync` subcommand enums.

use clap::Subcommand;

#[derive(Subcommand)]
pub enum SyncCommands {
    /// Show what push and pull would do, per session
    #[command(long_about = "Show what `agr sync push` and `agr sync pull` would do.

Read only, never prompts. Needs the network to list the remote sessions.
Prints one summary line per category, conflicts, failures and sessions
sealed with another key are always listed. Use -v for every session.

EXAMPLES:
    agr sync status       Summary per category
    agr sync status -v    One line per session")]
    Status {
        /// List every session, not only the summary
        #[arg(long, short = 'v', help = "List every session, not only the summary")]
        verbose: bool,
    },
    /// Upload new and changed sessions (encrypted)
    #[command(
        long_about = "Upload new and changed sessions, encrypted with your sync key.

Every push with planned changes asks for confirmation. Without a terminal
it needs --yes, otherwise nothing changes. The first push that creates gists
also shows a privacy notice.
Conflicts are skipped unless --overwrite is given (local wins).

EXAMPLES:
    agr sync push                    Push everything that changed
    agr sync push claude/a.cast.gz   Push one session
    agr sync push --dry-run          Show what would be pushed
    agr sync push -v                 Preview every session, not a summary"
    )]
    Push {
        /// Sessions to push (default: all)
        #[arg(help = "Sessions to push (default: all)")]
        sessions: Vec<String>,
        /// Show what would happen, change nothing
        #[arg(long, help = "Show what would happen, change nothing")]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip the confirmation prompt")]
        yes: bool,
        /// Resolve conflicts with the local session
        #[arg(long, help = "Resolve conflicts with the local session")]
        overwrite: bool,
        /// List every session, not only the summary
        #[arg(long, short = 'v', help = "List every session, not only the summary")]
        verbose: bool,
        /// Internal: push after a recording, never prompts
        #[arg(long, hide = true)]
        auto: bool,
    },
    /// Download new and changed sessions
    #[command(
        long_about = "Download remote sessions, decrypt and verify them before writing.

Needs your sync key (`agr sync key import` on a second machine).
Conflicts are skipped unless --overwrite is given (remote wins, the
replaced file is kept as a hidden .bak).

EXAMPLES:
    agr sync pull                    Pull everything that is new or changed
    agr sync pull --dry-run          Show what would be pulled
    agr sync pull -v                 Preview every session, not a summary"
    )]
    Pull {
        /// Sessions to pull (default: all)
        #[arg(help = "Sessions to pull (default: all)")]
        sessions: Vec<String>,
        /// Show what would happen, change nothing
        #[arg(long, help = "Show what would happen, change nothing")]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip the confirmation prompt")]
        yes: bool,
        /// Resolve conflicts with the remote session
        #[arg(long, help = "Resolve conflicts with the remote session")]
        overwrite: bool,
        /// List every session, not only the summary
        #[arg(long, short = 'v', help = "List every session, not only the summary")]
        verbose: bool,
    },
    /// Mark sessions deleted on the remote (encrypted bytes stay in the pack, the local file stays)
    #[command(
        long_about = "Mark a session deleted on the remote and drop its index entry. The encrypted bytes stay in the remote pack
gist until a prune, which is not part of sync yet.

The local file is never touched. The delete applies to the newest version of the session at the
time it runs, a version another machine pushed since the preview is deleted too. Patterns use * (any run) and ? (one character) against
<agent>/<name> of the remote sessions. More than 10 matches ask to type the count.
A pattern that matches no remote session aborts with exit 1 and deletes nothing, also when another
pattern matches. With --all on an empty remote it prints Nothing to delete. and exits 0.

EXAMPLE:
    agr sync delete claude/a.cast.gz
    agr sync delete 'claude/2024-*'
    agr sync delete --all --dry-run"
    )]
    Delete {
        /// Sessions to delete from the remote
        #[arg(
            required_unless_present = "all",
            help = "Patterns with * and ?, e.g. claude/*"
        )]
        sessions: Vec<String>,
        /// Delete every remote session
        #[arg(
            long,
            conflicts_with = "sessions",
            help = "Delete every remote session"
        )]
        all: bool,
        /// Allow more than 10 deletes without a terminal, a terminal still asks the count
        #[arg(
            long,
            help = "Allow more than 10 deletes without a terminal, a terminal still asks the count"
        )]
        force: bool,
        /// Show what would happen, change nothing
        #[arg(long, help = "Show what would happen, change nothing")]
        dry_run: bool,
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip the confirmation prompt")]
        yes: bool,
        /// List every session, not only the summary
        #[arg(long, short = 'v', help = "List every session, not only the summary")]
        verbose: bool,
    },
    /// Move the per-session gists of the old layout into packs
    #[command(long_about = "Move the per-session gists of the old layout into packs.

Shows what would move first and asks. Old gists are deleted only after the pack index
shows their session. Local session files are never touched. Run it by hand, sync refuses
to run until it has finished.

EXAMPLE:
    agr sync migrate
    agr sync migrate --yes")]
    Migrate {
        /// Skip the confirmation prompt
        #[arg(long, short = 'y', help = "Skip the confirmation prompt")]
        yes: bool,
    },
    /// Export or import the sync key
    #[command(
        subcommand,
        long_about = "Export or import the sync key.

SUBCOMMANDS:
    agr sync key export              Print the recovery phrase
    agr sync key import [PHRASE]     Install a recovery phrase (stdin when omitted)"
    )]
    Key(SyncKeyCommands),
}

#[derive(Subcommand)]
pub enum SyncKeyCommands {
    /// Print the recovery phrase of the sync key
    #[command(long_about = "Print the recovery phrase of the sync key.

Store it somewhere safe. Losing every copy of the key makes synced
sessions unrecoverable.

EXAMPLE:
    agr sync key export")]
    Export,
    /// Import a recovery phrase on this machine
    #[command(long_about = "Import a recovery phrase (argument or stdin).

Refuses to replace a different existing key without --force.
Prefer stdin, an argument ends up in your shell history.

EXAMPLES:
    agr sync key import              Read the phrase from stdin
    agr sync key import agr1-XXXXX-...")]
    Import {
        /// Recovery phrase (read from stdin when omitted, stdin avoids shell history)
        #[arg(help = "Recovery phrase (stdin when omitted, avoids shell history)")]
        phrase: Option<String>,
        /// Replace a different existing key
        #[arg(long, help = "Replace a different existing key")]
        force: bool,
    },
}
