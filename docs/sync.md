# Syncing Sessions

Sync copies your compressed sessions, encrypted, to secret GitHub gists so another machine can pull them. We need `gh` and `git` on the PATH and `gh auth login` done (the token needs the `gist` scope, `gh auth refresh -s gist` adds it). agr never reads or stores your GitHub token. How sync works inside (layers, pack format, encryption, push and pull flow) is in [architecture/sync.md](architecture/sync.md), the command reference is in [wiki/Command-sync.md](wiki/Command-sync.md).

## 1. What sync does

Many sessions are bundled into pack gists, so your gist page does not fill up with one entry per session. Every session is encrypted on your machine before it leaves, the remote only holds ciphertext.

- **Compressed only**: plain `.cast` files are skipped with a hint to run `agr compress`, sessions that are still recording are skipped, and so are sessions above 1 GiB on disk.
- **Nothing happens on its own**: `auto_push` is off by default, see section 9.
- **No cross deletes**: deleting a file locally never deletes it remotely and the other way round, only `agr sync delete` removes the remote copy.

## 2. Setup

The backend defaults to `gist`. To switch sync off set `backend = "off"` in `~/.config/agr/config.toml`:

```toml
[sync]
backend = "gist"      # default, "off" disables sync
auto_push = false     # default, true pushes each new recording
pack_size_mib = 64    # target size of one pack gist, 16 to 512
```

## 3. Migrate first

If you synced with an older version (one gist per session), run `agr sync migrate` before anything else. On a machine without a sync key run `agr sync key import` first (see section 8), migrate refuses without an existing key. Push, pull and delete refuse with "Run `agr sync migrate` first." until it has finished, and status prints the same hint.

```bash
agr sync migrate          # shows what would move, asks, then moves the sessions into packs
agr sync migrate --yes    # skip the question
```

An old gist is deleted only after the pack index shows its session, local files are never touched and re-running is safe. Without a terminal it needs `--yes`.

## 4. Push, pull and status

```bash
agr sync status                  # summary per category, read only
agr sync status -v               # one line per session
agr sync push                    # upload new and changed sessions, asks first
agr sync push claude/a.cast.gz   # one session
agr sync pull                    # download new and changed sessions
agr sync push --dry-run          # preview, change nothing
agr sync push --yes              # skip the question, needed without a terminal
```

Every push or pull with planned changes asks first. The first push that creates packs shows a privacy notice once per machine and creates your key (see section 8). A conflict (both sides changed, decided by hash and never by file time) is skipped and reported, `--overwrite` makes the local side win on push and the remote side win on pull (the replaced local file is kept as a hidden `.bak`). Pull checks every session against its hash before it replaces your file, so a damaged or tampered remote never touches the local copy.

## 5. Delete

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

## 6. Fork and `--overwrite`

When two machines push different content of one session at the same time, both pushes land and the next listing shows a **fork**. Push and pull skip it with "fork, run `agr sync push --overwrite`". Run `agr sync push --overwrite` on the machine whose version should win, the other version stays as dead data on the remote.

## 7. Deleted on the remote

A session that was deleted from the remote (by `agr sync delete` on another machine) shows as "deleted on the remote" in status and is not pushed back. Your local file stays. To restore it run `agr sync push --overwrite`, it uploads the local file again. If you edited the file after the last sync it counts as a fork and `--overwrite` resolves it the same way.

## 8. The key and what is stored where

Secret gists are unlisted to other users (you still see them on your own gist page), but **anyone who has the link can read them**, and recordings can hold secrets you typed in the terminal. That is why agr encrypts every session before it uploads.

- **On the remote**: ciphertext in pack gists, a description `agr:v2 <random id>`, file names and sizes, and commit times (UTC). Anyone with the link can see how many packs you have and, as the size of the `index` file per revision grows, roughly how many sessions each append added. They cannot see session content, session names or the agent.
- **`~/.config/agr/sync.key`**: your key, mode 0600, created by the first push only when the remote has no agr packs and no old agr gists. If it has either, agr stops and asks you to import the key of your other machine.
- **`~/.config/agr/state.toml`**: the privacy notice confirmation, and the last failed auto push.
- **Storage directory**: `.agr-sync.json` (what this machine last synced), `.agr-sync.lock` and `.agr-sync-queue`.

**Losing every copy of the key makes your synced sessions unrecoverable.** Back it up right after the first push:

```bash
agr sync key export             # prints the recovery phrase (agr1-XXXXX-...)
```

On a second machine:

```bash
agr sync key import             # paste the phrase on stdin, an argument also works but lands in your shell history
agr sync pull
```

`key import` refuses to replace a different existing key unless you pass `--force`. A gist keeps its git history, as such deleting the packs does not protect ciphertext that someone already copied. If the key is compromised, rotate it in this order:

1. Run `agr sync pull` on every machine and check `agr sync status`, so every remote session exists locally. Keeping local files alone does not save a session that only lives on the remote, and resolve any fork first.
2. Remove local files of sessions you deleted from the remote earlier. Deleting the old gists also deletes their tombstones, as such a leftover local file is pushed again as new.
3. Delete the old gists on GitHub yourself.
4. On every machine move `~/.config/agr/sync.key` and `.agr-sync.json` (in the storage directory) away. With the old key in place the next push would encrypt with it, and an old index with higher gens makes the rebuilt remote look like a rollback and is refused.
5. Push from one machine, the empty remote makes agr create a new key. Back it up with `agr sync key export`, import it on the other machines and pull there.

Ciphertext copied earlier stays readable with the old key.

## 9. Auto push

Once you set `auto_push = true`, a finished recording is pushed in the background, together with every session you synced before that changed since. Auto push never pulls or deletes, and a failure never changes the exit status of the recording. It waits until you confirmed the privacy notice with one manual `agr sync push`, until then the recording prints one hint line. A failed background push is shown as one warning on the next recording and by `agr sync status`.

## 10. Known limits

- GitHub lists at most 3000 gists, with that many sync aborts instead of guessing which packs exist.
- Pushes are paced and stop as soon as GitHub reports a rate limit, the first sync of many sessions may need several runs.
- A rename leaves the old name on the remote and on your other machines. `agr sync delete` removes the old remote entry only, so remove the old local file on each other machine yourself.
- A machine that lost its `.agr-sync.json` has no history to compare with, as such it cannot detect a rollback that happened before.
