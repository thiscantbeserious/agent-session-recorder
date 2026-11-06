# Requirements: Session Sync (GitHub Gists first)

GitHub issue: #164. Depends on #163 (gzip storage, merged in #165).

## Problem Statement

Sessions only live on one machine. We want to sync them between machines, with GitHub Gists as the first backend, but the design must allow other backends later (plain git remote, S3 or similar) without reworking the commands or the index.

## Desired Outcome

1. `agr sync push`, `agr sync pull` and `agr sync status` move compressed sessions between the local sessions folder and a remote backend.
2. Gists via the `gh` CLI are the only backend for now, so we reuse the user's GitHub auth and store no tokens ourselves.
3. A small `SyncBackend` trait (list remote, push, pull, delete) sits between the commands and the backend.
4. Conflicts are decided by content hash, never by mtime, and are never resolved silently.
5. Sync is off unless `[sync] backend = "gist"` is set. Nothing talks to the network otherwise.
6. The privacy risk (secret gists are reachable with the link, recordings can hold secrets) is stated clearly in the docs and on first push.

## Fixed Decisions (from issue #164 and the gzip work)

1. Backend trait with list remote, push, pull, delete. One implementation: Gists through the `gh` CLI.
2. Local sync index next to the sessions as a hidden dot file (same pattern as lock and backup files from #160). It tracks session path, content hash and remote id.
3. Conflicts are decided by hash, not by mtime.
4. Commands are `agr sync push`, `agr sync pull`, `agr sync status`.
10. Auto sync (user decision 2026-11-06): after every finished recording the new session is
    pushed automatically, enabled by default (`[sync] auto_push = true`) whenever a sync
    backend is configured. Push only, never pull, never delete. Without a configured backend
    nothing is synced.
5. Config is `[sync] backend = "gist"`, unset means sync is off. Added through a versioned migration (v5, current is v4).
6. Gists are created as secret by default.
7. Sync moves compressed files only (`<agent>/<name>.cast.gz`). Hashes are stable because gzip output is deterministic.
8. Machine-local state in `~/.config/agr/state.toml` is never synced.
9. Out of scope: other backends, auto pull, auto delete, background/watch sync.
11. Encryption (user decision 2026-11-06): every session is encrypted locally before upload,
    gists only ever hold ciphertext. Gists are always secret, never public. Reason: GitHub has
    no private gists, secret gists are readable by anyone with the URL.
12. Key storage (user decision 2026-11-06): the key lives in `~/.config/agr/sync.key` (mode
    0600, never synced, never printed except on explicit export). A second machine gets it
    via a recovery phrase: `agr sync key export` prints it once, `agr sync key import` reads
    it. GitHub secrets were considered and rejected: they are write-only for `gh`.

## Scope

### In Scope

**Config**
- New `[sync]` section with key `backend`, accepted value `"gist"`, unset means off. Invalid values are rejected with a clear message.
- Migration to v5 adds the section commented out or empty (sync stays off), running it twice changes nothing.
- `agr sync ...` with sync unset prints one clear line how to enable it and exits non-zero.

**Backend trait**
- `SyncBackend` with list remote, push, pull, delete. The commands only know the trait, the gist code is one implementation.
- The trait speaks in logical terms (session path, bytes, content hash, remote id), nothing gist specific leaks into the commands or the index.

**Gist layout and limits (constraint to be verified by the Architect)**
- Default is one gist per session, not one per agent. Reasons: 1. a single big session cannot block or fail the others, 2. per-session conflict and delete handling stays simple, 3. a per-agent gist would grow without bound and hit the size and file count limits sooner.
- Gists store text. The API does not take binary content, so the `.cast.gz` bytes cannot go in as a normal gist file. Default: push and pull through the gist's git repository (`gh gist clone`, then plain git), which holds binary files, so the file stays a real `.cast.gz` on the remote and `gzip -dc` works on a cloned gist. Fallback if the Architect finds that unworkable (for example git credentials for gists): base64 text in chunked files, about 33% larger, hash is always calculated over the original `.cast.gz` bytes either way.
- Known limits to verify and document: API responses truncate large files (raw content has to be fetched separately), git pushes reject single files over 100MB and warn from 50MB, and a gist has a file count limit. Our own limit is a hard cap per session on on-disk size, default 100MB. Larger sessions are skipped and reported with the reason, never half uploaded. (On the maintainer's data the largest session is 52MB compressed, so this covers the known data.)
- Each gist contains the session file plus one small metadata file (`.keep`, a neutral placeholder, the metadata itself is the sealed description token).

**Identity and discovery (second machine)**
- Every agr gist gets a fixed description marker, `agr:v1 <agent>/<name>`. `list remote` finds the user's agr gists by listing the authenticated user's gists and filtering on that prefix. No separate registry gist.
- A second machine with no local index builds it from the remote listing: same path and same hash is adopted as synced, same path with a different hash is a conflict, remote only is a pull candidate.
- Gists with the prefix but an unknown format version are ignored and reported as "unknown version", not touched.

**Sync index**
- Hidden dot file next to the sessions folder content, written atomically (temp file plus rename, hidden like all aux files). It is never treated as a session, never counted in stats, and never synced.
- Per session: path (`<agent>/<name>.cast.gz`), content hash of the last synced state, remote id, last sync time.
- The index is a cache of known sync state. If it is lost or corrupt, it is rebuilt from the remote listing and the local files with the same rules as a second machine, nothing is lost and nothing is overwritten silently.

**Sync decision per session (compare local hash, remote hash, last synced hash)**
1. Equal local and remote hash: in sync.
2. Only local changed since last sync, or local only: push candidate.
3. Only remote changed since last sync, or remote only: pull candidate.
4. Both changed and hashes differ: conflict. Skipped by both push and pull, reported with both hashes and sizes. Default is never overwrite. Explicit resolution with `--overwrite` on `push` (local wins) or on `pull` (remote wins), per run and optionally per session argument. Before pull overwrites a local file the old file is kept as the usual hidden `.bak`.

**Commands**
- `agr sync push [session...]`: pushes push candidates, optional session filter (same lookup rules as other commands).
- `agr sync pull [session...]`: pulls pull candidates, creates agent folders as needed, stores as `.cast.gz` regardless of the `compression` setting.
- `agr sync status`: read only, shows what push and pull would do, per session and as totals. Needs network (lists remote), prints a clear message if not reachable.
- `--dry-run` on push and pull: same output as the real run, shows what would happen with sizes, changes nothing locally or remotely.
- `--yes` skips the confirmation prompt, without a TTY there is no prompt (pattern from `agr compress`), a non-interactive run needs `--yes` to push or pull or it exits with a clear message and changes nothing. `status` and `--dry-run` never prompt.
- Output goes through the theme helpers and respects `NO_COLOR`.
- Per-file failures do not abort the run, they are reported in the summary (pushed, pulled, skipped, conflicts, failed, bytes).

**Skipping rules**
- Plain `.cast` sessions are not synced. They are skipped and reported with the hint to run `agr compress`.
- Sessions with a live lock (running PID) are skipped and reported. Stale locks are handled as today.
- Hidden aux files (`.bak`, `.lock`, temp files, the index) are never sessions and never synced.

**Deletes**
- Deleting a session locally (`agr cleanup`, manual) never deletes it remotely, and a remote deletion never deletes the local file.
- Remote delete is explicit only: `agr sync delete <session>` (uses the backend `delete`), asks for confirmation unless `--yes`, supports `--dry-run`. It removes the gist and the index entry, the local file stays.
- A session that is in the index but missing locally shows up in `status` as "remote only" and is pulled again on the next `pull`. That is the expected result of "no implicit deletes", and `agr sync delete` is how to end it.

**Privacy**
- Gists are always created secret. There is no setting for public in this version.
- Docs (README and `src/config/docs.rs`) state clearly: a secret gist is not listed publicly but anyone with the link can read it, and recordings can contain secrets typed in the terminal (tokens, passwords, environment output). Do not sync sessions you would not hand to someone holding the link.
- First `push` that would create a remote gist (index has no remote ids and no remote agr gists exist yet) prints this warning and requires a confirmation, `--yes` skips it, no TTY without `--yes` aborts. The acknowledgement is stored in `~/.config/agr/state.toml` (machine-local), so it is asked once per machine.

**Missing or unauthenticated `gh`**
- Before any network step, check `gh` is on PATH and `gh auth status` succeeds. If not, print one clear message each ("gh not found, install from https://cli.github.com" and "gh not logged in, run `gh auth login`"), exit non-zero, change nothing.
- The token needs the `gist` scope. A missing scope gives a message naming it (`gh auth refresh -s gist`).
- agr never reads, prints, logs or stores the GitHub token.

### Out of Scope

- Encryption (client side or otherwise).
- Other backends (git remote, S3). The trait only has to leave room.
- Auto pull, auto delete, background sync, watch mode (auto push after record is in scope).
- Public gists or any visibility setting.
- Syncing plain `.cast`, `.bak`, lock files, the index or `state.toml`.
- Merging two diverged sessions. Conflicts are resolved by picking one side.
- Syncing config or other users' gists, sharing a session by link.
- Windows support.

## Acceptance Criteria

**Config**
- [ ] A config without `[sync]` is migrated to v5 and sync stays off. Running the migration twice changes nothing. An invalid `backend` value gives a clear error.
- [ ] With sync unset, `agr sync push|pull|status` print how to enable sync and exit non-zero, and no `gh` process is started.

**Gh preconditions**
- [ ] With `gh` missing from PATH, every sync command prints the install hint, exits non-zero and changes nothing.
- [ ] With `gh` not logged in, every sync command prints the login hint, exits non-zero and changes nothing.
- [ ] No agr output, log or file contains the GitHub token.

**Push**
- [ ] `agr sync push` creates one secret gist per new session, with description `agr:v1 <agent>/<name>`, the `.cast.gz` bytes as `data.NNN` parts and a `.keep` placeholder.
- [ ] The remote file is the byte-identical `.cast.gz` (hash equal to the local hash) or, on the base64 fallback, decodes to it.
- [ ] After a push the index holds path, hash and remote id, and a second push reports nothing to do.
- [ ] A session changed locally after the last sync updates the existing gist, no second gist is created.
- [ ] Sessions above the size cap are skipped with a reason and the rest is pushed.
- [ ] Plain `.cast` sessions and sessions with a live lock are skipped and listed with a reason. Hidden aux files are never pushed.
- [ ] `--dry-run` lists what would be pushed with sizes, and creates no gist and does not touch the index.
- [ ] First push that creates gists shows the privacy warning and needs confirmation, `--yes` skips it, no TTY without `--yes` aborts without changes. The warning is not repeated on later pushes on the same machine.

**Pull**
- [ ] `agr sync pull` on a machine with no local sessions and no index downloads all agr gists to `<agent>/<name>.cast.gz` and builds the index.
- [ ] Pulled files are byte-identical to the pushed ones (same hash) and open with `play`, `analyze` and `ls`.
- [ ] A remote-only session is pulled, a local-only session is not touched by pull.
- [ ] A truncated or corrupt download (hash mismatch with the metadata) is rejected, no partial file is left behind, and it is reported as failed.
- [ ] Pull writes through a hidden temp file and atomic rename, an existing local file is never overwritten without `--overwrite`.
- [ ] `--dry-run` lists what would be pulled with sizes and changes nothing.
- [ ] Gists without the `agr:v1` prefix are never listed or touched. Gists with an unknown format version are reported and ignored.

**Conflicts**
- [ ] Same path, different hash on both sides, both changed since last sync: reported as conflict with both hashes, neither push nor pull changes either side.
- [ ] Same path, different hash on a fresh machine (no index): reported as conflict, nothing overwritten.
- [ ] Hash decides, not mtime: a local file with a newer mtime but the remote changed since last sync is still a conflict. Equal hashes with different mtimes are in sync.
- [ ] `push --overwrite` makes local win, `pull --overwrite` makes remote win, and pull keeps the replaced local file as hidden `.bak`.

**Status**
- [ ] `agr sync status` shows per session: in sync, local ahead (push), remote ahead (pull), conflict, remote only, local only, skipped (plain or locked), with totals per state.
- [ ] It shows the backend and number of remote sessions, uses theme helpers, and respects `NO_COLOR`.
- [ ] It changes nothing locally or remotely, and never prompts.

**Deletes**
- [ ] `agr cleanup` deleting a synced session does not touch the gist. Status then shows it as remote only.
- [ ] Deleting a gist outside agr makes status show the session as local only, and nothing is deleted locally.
- [ ] `agr sync delete <session>` asks for confirmation (not without `--yes`), removes the gist and the index entry and keeps the local file. `--dry-run` changes nothing.

**Index**
- [ ] The index is a hidden file, never listed as a session, never counted in `agr status` sizes, never pushed.
- [ ] A deleted or corrupt index is rebuilt from remote plus local files and results in the same decisions as a second machine, with no overwrites.
- [ ] Index writes are atomic (temp plus rename).

**Non-interactive**
- [ ] Without a TTY, push and pull without `--yes` abort with a message and change nothing, `status` and `--dry-run` work.

**Robustness**
- [ ] A failure on one session (network, rate limit, size) does not stop the others and is reported in the final summary.
- [ ] The backend is used only through the `SyncBackend` trait in the commands and the sync logic, and the sync logic is testable with an in-memory fake backend without `gh` or network.

**Quality**
- [ ] `cargo test`, `cargo clippy` and `cargo fmt --check` pass.
- [ ] README and `src/config/docs.rs` describe `[sync]`, the three commands, the gist layout, the size cap and the privacy statement (secret gist is reachable with the link, recordings can hold secrets).
- [ ] Real gist calls are not part of the default test run (fake backend in unit tests, any live test is ignored by default).

**Auto push after recording** (user decision 2026-11-06)
- [ ] With `[sync] backend = "gist"` and no `auto_push` key, a finished recording is pushed
  automatically after compression (only that session, same code path as `agr sync push`).
- [ ] `auto_push = false` disables it; without a configured backend nothing is pushed.
- [ ] Auto push never pulls and never deletes, locally or remotely.
- [ ] Auto push failure (no network, `gh` missing, not logged in, size cap, conflict) prints
  one warning line and never changes the recording's exit status or blocks the shell.
- [ ] Auto push never shows the privacy prompt: until the first-push acknowledgement is stored
  in `state.toml`, auto push is skipped with a one-line hint to run `agr sync push` once.
- [ ] Live-locked and plain `.cast` sessions are never auto pushed.
- [ ] Config migration adds `auto_push = true` in the `[sync]` section template; the docs state
  that auto push is on by default once a backend is set.

**Encryption and key** (user decision 2026-11-06)
- [ ] Every uploaded gist file is ciphertext (authenticated encryption, e.g. XChaCha20-Poly1305
  or AES-256-GCM, chosen and justified in the ADR), the plaintext `.cast.gz` never leaves the
  machine. A test asserts the uploaded bytes do not contain the gzip magic or any session text.
- [ ] Pull decrypts and verifies authenticity before writing; a tampered or wrong-key file is
  rejected with a clear error naming the session, nothing is written.
- [ ] First `agr sync push` without a key generates one, stores it in `~/.config/agr/sync.key`
  with mode 0600 (refuses a key file readable by others), and prints that the key must be
  backed up with `agr sync key export`.
- [ ] `agr sync key export` prints a recovery phrase (human-typable, includes a checksum);
  `agr sync key import` accepts it, rejects a phrase with a bad checksum, refuses to overwrite
  an existing different key without `--force`.
- [ ] Pulling on a machine without a key aborts before any change with a hint to import it.
- [ ] Remote metadata (gist description, metadata file) contains no session content; the
  session name in the description is acceptable, or is encrypted too (ADR decides, autonomous).
- [ ] Docs state clearly: losing every copy of the key makes synced sessions unrecoverable.
- [ ] Hash-based conflict detection and the index use the plaintext hash, so deterministic
  gzip output still yields stable comparisons despite random nonces in the ciphertext.

**Large sessions and listing limit** (user decision 2026-11-06)
- [ ] The gist backend splits the sealed blob into parts of at most 45 MiB
  (`data.000`, `data.001`, ...), all in the SAME gist as the session, pushed in
  one commit. A blob under 45 MiB is a single part `data.000`.
- [ ] Pull fetches all parts of the gist, joins them in order and only then opens and verifies
  the blob; a missing, extra or reordered part is rejected, nothing is written.
- [ ] Re-pushing a session whose part count shrinks removes the stale higher parts in the same
  commit.
- [ ] The per-session cap is 1 GiB of sealed size (was 100 MB), sessions above are skipped
  with a reason. Splitting lives only in the gist backend, the engine and `SyncBackend` trait
  still move one blob.
- [ ] If the gist listing returns 3000 or more entries, sync aborts before any change with a
  clear error (GitHub documents a 3000 gist pagination cap), never treats unlisted gists as
  missing.

## Constraints

- Sync works on `.cast.gz` bytes as stored, no recompression, so the hash stays stable.
- No tokens in agr config, state or logs. All auth goes through `gh`.
- Config changes go through the versioned migration system (v5).
- Hidden-file convention from #160 applies to the index and all temp files.
- Machine-local acknowledgement state goes to `~/.config/agr/state.toml`, never synced.
- Gist limits and the binary handling path are an Architect decision and need to be verified against the real `gh` and GitHub behaviour before the ADR.
- No new heavy dependencies, the `gh` CLI is called as an external process, hashing needs a SHA-256 implementation (Architect picks, pure Rust preferred, check what is already in Cargo.toml first).

## Context

- Verified from the gzip feature: sessions are `<agent>/<name>.cast.gz`, plain `.cast` is still possible (`compression = "none"` or old sessions), gzip output is deterministic, exact uncompressed size is in the gzip header, hidden aux files are never sessions, live-locked sessions exist as hidden lock files.
- `agr compress` already has the `--yes` and `--dry-run` pattern and the no-TTY behavior, sync mirrors it.
- Gist API limits (truncation of large files in responses, no binary content through the API, 100MB per file in git) are from memory and not verified in this phase.

## Decisions taken autonomously (review in PR)

- Auto push is enabled by default but only when a backend is configured, sync itself stays
  opt-in (user asked for auto sync enabled by default, push only; uploading recordings that can
  hold secrets without ever choosing a backend would be unsafe). It also waits for the one-time
  privacy acknowledgement.

1. **One gist per session, not per agent**: independent failures and conflicts, no unbounded growth, avoids the gist file count and size limits.
2. **Binary handling via the gist git repo, base64 chunks as fallback**: keeps the remote file a real `.cast.gz` and the hash identical, the Architect verifies feasibility (git credentials for gists) and may flip to the fallback.
3. **Size cap of 100MB on-disk per session, skip and report above**: matches the git per-file limit, covers the largest known session (52MB), no config key for it yet.
4. **Identity by description marker `agr:v1 <agent>/<name>` plus `agr-session.json` metadata**: a second machine finds the gists by listing the user's gists, no extra registry, version in the marker allows format changes later.
5. **Conflict means skip and report, resolved only with `--overwrite` (local wins on push, remote wins on pull)**: never lose data silently, pull keeps a `.bak` of the replaced file.
6. **Three-way decision using the last synced hash from the index**: this is what makes "both changed" detectable by hash, without mtime.
7. **No implicit deletes in either direction, explicit `agr sync delete <session>`**: cleanup is a local space tool, and a surprise remote delete is not recoverable.
8. **Plain `.cast` is skipped with a hint to `agr compress`**: sync moves compressed files only (issue), no hidden recompression.
9. **Pulled sessions are always stored as `.cast.gz`**: that is what is on the remote, independent of the `compression` setting.
10. **Live-locked sessions are skipped**: a file still being written has no stable hash.
11. **`status` is read only, needs network, shows per-state list and totals**: it has to compare with the remote to be useful.
12. **`--dry-run` on push, pull and delete, `--yes` and no-TTY behavior mirror `agr compress`**: same pattern users already know, no prompt without a TTY.
13. **Privacy warning plus confirmation on first creating push, acknowledgement stored per machine in `state.toml`**: it is the one moment the risk becomes real, and asking once avoids nagging.
14. **Missing `gh`, no login or missing `gist` scope abort before any change with one clear message each**: no partial state, agr never touches the token.
15. **Secret only, no visibility setting**: the issue says secret by default, a public option adds risk with no need yet.
16. **Index rebuildable from remote plus local, and unknown format versions ignored**: the index is a cache, losing it must not cause data loss.
17. **Sync logic tested with an in-memory fake backend, live gist tests ignored by default**: keeps the default test run offline.
18. **Index file name and format left to the Architect**: only the hidden-dot-file rule is fixed here.

---
**Sign-off:** Auto-approved (user delegated, review in PR)
