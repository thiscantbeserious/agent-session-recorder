# agr sync

Sync encrypted sessions to a remote (secret gists)

## 1. Before you start

**Purpose**: copy your compressed sessions to a second place so another machine can pull them. We need `gh` and `git` on the PATH and `gh auth login` with the `gist` scope. Sessions are encrypted on your machine, how it works inside is in [docs/architecture/sync.md](../architecture/sync.md).

**1.1 Migrate first**: if you synced with an older version (one gist per session), run `agr sync migrate` before anything else. Push, pull, delete and the auto push child refuse with "Run `agr sync migrate` first." until it has finished, and status prints the same hint.

```bash
agr sync migrate          # shows what would move, asks, then moves the sessions into packs
agr sync migrate --yes    # skip the question
```

The first run prints `N sessions to move (size), M already moved, K empty stubs to delete, L not ours (kept)` and asks `Move these sessions into packs and delete the old gists? Local files stay. [y/N]`. Without a terminal it needs `--yes`. An old gist is deleted only after the pack index shows its session, local files are never touched. Re-running is safe.

## 2. Push, pull and status

```bash
agr sync status                  # summary per category, read only
agr sync status -v               # one line per session
agr sync push                    # upload new and changed sessions, asks first
agr sync push claude/a.cast.gz   # one session
agr sync pull                    # download new and changed sessions
agr sync push --dry-run          # preview, change nothing
agr sync push --yes              # skip the question, needed without a terminal
```

Every push or pull with planned changes asks first. The first push that creates packs shows a privacy notice once per machine. A conflict (both sides changed, decided by hash and never by file time) is skipped and reported, `--overwrite` makes the local side win on push and the remote side win on pull (the replaced file is kept as a hidden `.bak`).

## 3. Delete

`agr sync delete` removes the remote copy, the local file always stays. Patterns use `*` (any run) and `?` (one character) against `<agent>/<name>`.

```bash
agr sync delete claude/a.cast.gz
agr sync delete 'claude/2024-*'
agr sync delete --all --dry-run
```

- **`--all`**: every remote session, cannot be combined with patterns.
- **`--yes`**: skips the yes or no question only.
- **More than 10 matches**: a terminal always asks you to type the count, also with `--yes` and `--force`.
- **`--force`**: only lifts the abort when there is no terminal, without it more than 10 deletes without a terminal stop.

Deleted sessions become tombstones, the bytes are freed later.

## 4. Fork and `--overwrite`

When two machines push different content of one session at the same time, both pushes land and the next listing shows a **fork**. Push and pull skip it with "fork, run `agr sync push --overwrite`". Run `agr sync push --overwrite` on the machine whose version should win, it is pushed one gen higher and the other version stays as dead data.

## 5. Deleted on the remote

A session that was deleted from the remote (by `agr sync delete` on another machine) shows as "deleted on the remote" in status and is not pushed back. Your local file stays. To restore it run `agr sync push --overwrite`, it uploads the local file again. If you edited the file after the last sync it counts as a fork and `--overwrite` resolves it the same way.

## 6. Configuration

`[sync] pack_size_mib` sets the target size of one pack gist, default 64, range 16 to 512, see [Configuration](Configuration).

## 7. Reference

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

Mark sessions deleted on the remote (encrypted bytes stay in the pack, the local file stays)

Mark a session deleted on the remote and drop its index entry. The encrypted bytes stay in the remote pack
gist until a prune, which is not part of sync yet.

The local file is never touched. The delete applies to the newest version of the session at the
time it runs, a version another machine pushed since the preview is deleted too. Patterns use * (any run) and ? (one character) against
<agent>/<name> of the remote sessions. More than 10 matches ask to type the count.
When nothing matches it prints Nothing to delete. and exits 0.

EXAMPLE:
    agr sync delete claude/a.cast.gz
    agr sync delete 'claude/2024-*'
    agr sync delete --all --dry-run

### sync migrate

Move the per-session gists of the old layout into packs

Move the per-session gists of the old layout into packs.

Shows what would move first and asks. Old gists are deleted only after the pack index
shows their session. Local session files are never touched. Run it by hand, sync refuses
to run until it has finished.

EXAMPLE:
    agr sync migrate
    agr sync migrate --yes

### sync key

Export or import the sync key

Export or import the sync key.

SUBCOMMANDS:
    agr sync key export              Print the recovery phrase
    agr sync key import [PHRASE]     Install a recovery phrase (stdin when omitted)

