## 1. Before you start

**Purpose**: copy your compressed sessions to a second place so another machine can pull them. We need `gh` and `git` on the PATH and `gh auth login` with the `gist` scope. Sessions are encrypted on your machine, how it works inside is in [docs/architecture/sync.md](https://github.com/thiscantbeserious/agent-session-recorder/blob/main/docs/architecture/sync.md).

**1.1 Migrate first**: if you synced with an older version (one gist per session), run `agr sync migrate` before anything else. On a machine without a sync key import it first with `agr sync key import`, migrate refuses without an existing key (the phrase comes from `agr sync key export` on your other machine, see section 2). Push, pull, delete and the auto push child refuse with "Run `agr sync migrate` first." until it has finished, and status prints the same hint.

```bash
agr sync migrate          # shows what would move, asks, then moves the sessions into packs
agr sync migrate --yes    # skip the question
```

The first run prints `N sessions to move (size), M already moved, K empty stubs to delete, L not ours (kept)` and asks `Move these sessions into packs and delete the old gists? Local files stay. [y/N]`. Without a terminal it needs `--yes`. An old gist is deleted only after the pack index shows its session, local files are never touched. Re-running is safe.

## 2. Push, pull and status

A second machine needs the original sync key. **Losing every copy of it makes the encrypted remote sessions unrecoverable.** Right after the first push run `agr sync key export` and store the printed recovery phrase (`agr1-XXXXX-...`) somewhere safe, outside the machine. On the second machine run `agr sync key import`, paste the phrase on stdin, and then pull.

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

A delete appends a tombstone. The encrypted bytes stay in the pack gists and in their git history, nothing frees them, so treat a deleted session as still held remotely (as ciphertext).

## 4. Fork and `--overwrite`

When two machines push different content of one session at the same time, both pushes land and the next listing shows a **fork**. Push and pull skip it with "fork, run `agr sync push --overwrite`". Run `agr sync push --overwrite` on the machine whose version should win, it is pushed one gen higher and the other version stays as dead data.

## 5. Deleted on the remote

A session that was deleted from the remote (by `agr sync delete` on another machine) shows as "deleted on the remote" in status and is not pushed back. Your local file stays. To restore it run `agr sync push --overwrite`, it uploads the local file again. If you edited the file after the last sync it counts as a fork and `--overwrite` resolves it the same way.

## 6. Configuration

`[sync] pack_size_mib` sets the target size of one pack gist, default 64, range 16 to 512, see [Configuration](Configuration).

## 7. Reference
