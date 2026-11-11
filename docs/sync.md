# Syncing Sessions

Sync copies your compressed sessions to a second place so another machine can pull them. Right now the only backend is secret GitHub gists through the `gh` CLI, as such `gh` and `git` need to be on your PATH and `gh auth login` done (the token needs the `gist` scope, `gh auth refresh -s gist` adds it). agr never reads or stores your GitHub token.

How sync works inside (layers, remote layout, encryption, push and pull flow) is described in [docs/architecture/sync.md](architecture/sync.md).

## Setup

The backend defaults to `gist` but nothing is uploaded on its own, `auto_push` is off by default. A push needs `gh` and `git` and the privacy notice confirmed with a first `agr sync push`. To switch sync off set `backend = "off"` in `~/.config/agr/config.toml`:

```toml
[sync]
backend = "gist"   # default, "off" disables sync
auto_push = false  # default, true pushes each new recording
```

```bash
agr sync status                 # What push and pull would do, per session, read only
agr sync push                   # Upload new and changed sessions
agr sync pull                   # Download new and changed sessions
agr sync delete claude/a.cast.gz  # Mark the remote copy deleted, the local file stays
agr sync push --dry-run         # Preview, never prompts or changes anything
agr sync push --yes             # Skip the confirmation (needed without a terminal)
```

Sessions (`agent/name.cast.gz`) are appended into pack gists of about `pack_size_mib` each. Plain `.cast` files are skipped with a hint to run `agr compress`, sessions that are still recording are skipped, and so are sessions above 1 GiB. The cap applies to the local `.cast.gz` size on disk, not to the encrypted size. GitHub lists at most 3000 gists, with that many sync aborts instead of guessing which sessions exist. A conflict (both sides changed, decided by content hash and never by file time) is skipped and reported, `--overwrite` makes the local side win on push and the remote side win on pull (the replaced local file is kept as a hidden `.bak`). Deleting a session locally never deletes it remotely and the other way round, only `agr sync delete` marks the remote copy deleted (its encrypted bytes stay in the remote pack). A later plain `agr sync push` uploads a deleted session again, as the local file is still there.

## How it works across machines

Every session gist holds an encrypted `manifest` and the encrypted `data.NNN` parts, committed together. The manifest carries the session name, the content hash and a generation counter that goes up by one on every push. Pull compares that counter with the last one this machine saw.

Anyone who can see the gist can see:

- the `agr:v1` prefix in the description, the file names (`manifest`, `data.000`, ...) and their sizes
- the commit times (always UTC)
- how many sessions you have synced

They cannot see the session content, the session names or which agent recorded them.

## Push and pull safety

Every push is a compare-and-swap. If the gist changed on the remote since agr looked at it, the session is reported as "changed on the remote, run again" and nothing is written. Run the sync again and it plans against the new state. Nothing on the remote is overwritten without `--overwrite`, and agr never force pushes. An interrupted upload is resumed by the next push.

Pull refuses a remote version that is older than the last one this machine synced (rollback), and a manifest or blob that belongs to another session or does not match its hash. The local file stays untouched in all of these cases.

## Memory and tampering

Sessions are encrypted and decrypted in chunks. Each 64 KiB chunk is encrypted and authenticated on its own, as such memory stays small however big the session is. A damaged or tampered gist (cut off, reordered, with chunks dropped, duplicated or appended) is refused, and your local file is not touched. Pull writes into a hidden temp file first and only replaces your file once the whole stream and its hash checked out.

## Duplicates, deleted gists and rate limits

- **Duplicates**: if the same session exists in more than one gist, agr uses one of them, reports the others with `gh gist delete <id>` and never deletes anything itself.
- **Deleted gist**: if you deleted the gist on GitHub, the session is reported as "remote copy gone" and skipped. Only `agr sync push --overwrite` uploads it again.
- **Rate limits**: agr paces its changing calls to at most 60 per minute and stops as soon as GitHub reports a rate limit, with "N left, please run again later". The first sync of many sessions may need several runs.

## Known limits

- GitHub lists at most 3000 gists, with that many sync aborts.
- The gist description can lag behind the manifest after an interrupted push. A later run repairs it.
- A rename leaves the old name on your other machines until you run `agr sync delete` for it there.
- If the key is compromised, every old version in the gist history is exposed. Deleting the gists is the only remedy.
- When two machines push the same session at nearly the same time, GitHub has no conditional description update, so a newer description can be overwritten in a window of a few milliseconds. The next run of the newer machine repairs it.
- `agr sync delete` racing a push of the same session on another machine is not a compare-and-swap. The other machine creates the gist again from its local copy on its next push.
- A push landed but the run crashed before the index save, and the file was edited before the next run: it is reported as a conflict, `--overwrite` resolves it.
- A machine that lost its index (`.agr-sync.json` in the storage directory) trusts the first version it sees, so it cannot detect a rollback that happened before.

## Encryption and the key

Secret gists are not listed anywhere, but **anyone who has the link can read them**, and recordings can hold secrets you typed in the terminal. That is why agr encrypts every session on your machine before it uploads (XChaCha20-Poly1305). The gist only holds ciphertext, and its description only holds an opaque token, so session names are not readable either. A gist keeps its git history, as such old ciphertexts of a changed session stay readable to anyone with the link and the key.

The first push that creates gists shows this notice and asks for a confirmation once per machine (stored in `~/.config/agr/state.toml`). The key is created then, in `~/.config/agr/sync.key` with mode 0600, but only if the remote has no agr gists yet. If it has, agr stops and asks you to import the key of your other machine.

**Losing every copy of the key makes your synced sessions unrecoverable.** Back it up right after the first push:

```bash
agr sync key export             # Prints the recovery phrase (agr1-XXXXX-...)
```

On a second machine:

```bash
agr sync key import             # Paste the phrase on stdin, an argument also works but lands in your shell history
agr sync pull
```

`key import` refuses to replace a different existing key unless you pass `--force`. Remote entries sealed with another key are reported and left alone, remove them with `gh gist delete <id>` if you do not need them.

## Test-only backend switch

`AGR_SYNC_TEST_BACKEND_DIR=<dir>` redirects sync to a local folder instead of GitHub. It exists for the end to end tests and stays in release builds. It is for tests only, never set it for real data. Every sync command prints `Using test sync backend at <dir>` on stderr when it is set.

## Auto push

Auto push is off by default (`auto_push = false`). Once you turn it on, a finished recording is pushed in the background. The new session is pushed together with every session you synced before that changed since (markers, optimize), a new session that cannot be pushed right away because another sync holds the lock waits in a small queue (`.agr-sync-queue` in the storage directory, shown by `agr sync status`) and the running sync or the next one takes it, and a rename is only picked up by a manual push. Auto push never pulls or deletes, and a failure never changes the exit status of the recording. Auto push waits until you confirmed the privacy notice with one manual `agr sync push`, until then the recording prints one hint line. A failed background push is stored in `state.toml`, shown as one warning on the next recording and by `agr sync status`.

To turn it on:

```toml
[sync]
auto_push = true
```
