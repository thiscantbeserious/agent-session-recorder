# agr sync

Sync encrypted sessions to a remote (secret gists)

## Usage

```
agr sync [OPTIONS]
```

## Subcommands

### sync status

Show what push and pull would do, per session

Show what `agr sync push` and `agr sync pull` would do.

Read only, never prompts. Needs the network to list the remote sessions.
Prints one summary line per category, conflicts, failures and sessions
sealed with another key are always listed. Use -v for every session.

EXAMPLES:
    agr sync status       Summary per category
    agr sync status -v    One line per session

### sync push

Upload new and changed sessions (encrypted)

Upload new and changed sessions, encrypted with your sync key.

Every push with planned changes asks for confirmation. Without a terminal
it needs --yes, otherwise nothing changes. The first push that creates gists
also shows a privacy notice.
Conflicts are skipped unless --overwrite is given (local wins).

EXAMPLES:
    agr sync push                    Push everything that changed
    agr sync push claude/a.cast.gz   Push one session
    agr sync push --dry-run          Show what would be pushed
    agr sync push -v                 Preview every session, not a summary

### sync pull

Download new and changed sessions

Download remote sessions, decrypt and verify them before writing.

Needs your sync key (`agr sync key import` on a second machine).
Conflicts are skipped unless --overwrite is given (remote wins, the
replaced file is kept as a hidden .bak).

EXAMPLES:
    agr sync pull                    Pull everything that is new or changed
    agr sync pull --dry-run          Show what would be pulled
    agr sync pull -v                 Preview every session, not a summary

### sync delete

Remove sessions from the remote (the local file stays)

Remove the remote copy of a session and its index entry.

The local file is never touched.

EXAMPLE:
    agr sync delete claude/a.cast.gz

### sync key

Export or import the sync key

Export or import the sync key.

SUBCOMMANDS:
    agr sync key export              Print the recovery phrase
    agr sync key import [PHRASE]     Install a recovery phrase (stdin when omitted)

