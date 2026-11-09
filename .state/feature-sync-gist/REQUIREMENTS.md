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
    (2026-11-07: user decision, changed synced sessions go with the next recording. The auto
    push sends the new session and every session that has an index entry and changed locally
    since, never a session that was not synced before, and a rename waits for a manual push.)
5. Config is `[sync] backend = "gist"`, unset means sync is off. Added through a versioned migration (v5, current is v4).
   (Update 2026-11-06: became v6 because auto-optimize took v5 on main.)
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
- Known limits to verify and document: API responses truncate large files (raw content has to be fetched separately), git pushes reject single files over 100MB and warn from 50MB, and a gist has a file count limit. Our own limit is a hard cap per session on on-disk size, default 100MB (raised to 1 GiB later, see the large sessions items). Larger sessions are skipped and reported with the reason, never half uploaded. (On the maintainer's data the largest session is 52MB compressed, so this covers the known data.)
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
- [ ] The per-session cap is 1 GiB of local `.cast.gz` size (was 100 MB), sessions above are skipped
  with a reason. Splitting lives only in the gist backend, the engine and `SyncBackend` trait
  still move one blob.
- [ ] If the gist listing returns 3000 or more entries, sync aborts before any change with a
  clear error (GitHub documents a 3000 gist pagination cap), never treats unlisted gists as
  missing.

**Robust multi-machine sync, Variant A** (user decision 2026-11-06, after adversarial review)
- [ ] Each session gist holds an encrypted `manifest` in the same commit as its parts:
  `{v: 2, path, sha256, size, gen}` (no part count, D2); the commit is the source of truth, the description
  is only a listing hint and is repaired from the manifest when they disagree.
- [ ] `gen` starts at 1 and increments on every push; parts are bound to path and gen (AAD),
  so old parts can never be paired with a newer manifest. The blob header is part of the AAD.
- [ ] Push is a compare-and-swap: the manifest in the fresh clone must match the remote state
  the plan was based on, otherwise the session is reported as changed remotely and nothing is
  written; git push is never forced, a non-fast-forward rejection is reported as a conflict.
- [ ] Pull rejects a manifest whose gen is lower than the last synced gen in the index
  (rollback from gist history), the local file stays untouched.
- [ ] Interrupted pushes recover: data pushed but description stale -> repaired on next run;
  new gist left as `agr:pending` -> found via the index remote id and completed, never orphaned.
- [ ] Duplicates are prevented by pushing to the remembered index remote id when the listing
  misses it; remaining duplicates are never deleted automatically (D1): the oldest by server `created_at`
  (id as tie-break) is used, the others are reported in status.
- [ ] Rate limits: GitHub 403/429/"rate limit" responses map to a distinct error that stops
  the whole run cleanly with "N left, run again later", no automatic retry (D4); mutating
  calls are paced to at most 60 per minute, no own hourly budget (D3).
- [ ] Commit author and committer dates are written in UTC (+0000), identity stays neutral.
- [ ] Known limits documented, not fixed: 3000 gist listing cap, session count and timing
  visible, rename leaves the old name on other machines until `agr sync delete`, key compromise
  exposes gist history.

**Batch 12 review decisions** (user decisions 2026-11-06)
- [x] D6: a session whose gist is gone on GitHub (NotFound from the API) is reported as
  "remote copy gone" and skipped; push never recreates it on its own, only
  `agr sync push --overwrite` does. Local file and index entry stay.
- [x] Auth or access failures that git reports as "Repository not found" are never treated as
  a deleted gist (only a `gh` API 404 is NotFound), so a wrong token can never cause a second
  gist.
- [x] D7: a listed gist without `created_at` never wins the duplicate pick (fail closed).
- [x] D8: a duplicate pick may change once when a formerly pending gist gets its description;
  both copies are reported, nothing is deleted or overwritten.

**PO acceptance follow-ups** (user decisions 2026-11-06)
- [ ] D9: encryption and decryption stream in chunks, so peak memory stays bounded and flat
  regardless of session size (up to the 1 GiB cap). The sealed format stays authenticated end
  to end: truncation, reordering, dropping or duplicating chunks is detected, nothing is
  written on pull unless the whole stream verified. A performance test (ignored by default)
  documents peak memory for a large session.
- [ ] D10: the test-only backend switch `AGR_SYNC_TEST_BACKEND_DIR` stays in the release
  binary (e2e needs it), announces itself on stderr, and is documented in the PR and README
  developer notes as test-only.
- [ ] README states that the 1 GiB cap applies to the local `.cast.gz` size.

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
3. **Size cap of 100MB on-disk per session (now 1 GiB), skip and report above**: matches the git per-file limit, covers the largest known session (52MB), no config key for it yet.
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

**2026-11-09:** user decision, backend defaults to gist, `off` disables. Supersedes the autonomous decision to keep it unset.

**2026-11-09:** user decision, auto_push off by default, nobody is forced to upload.

---

## Pack layer (2026-11-09)

**Problem**: one secret gist per session does not scale. The first real push created about 60 `.keep` gists (stubs from the stopped push) and buried the gist page of the user. Every changed session also rewrites a whole gist, and git history keeps the old bytes anyway.

**Outcome**: many sessions live in a few big encrypted packs, the backend is a dumb pack store, and push, pull, status and auto push do not get worse for the user.

**Sources**: ADR section "Pack layer (2026-11-09)", PLAN Stages 46 to 61, `docs/architecture/sync.md` (current per-session behaviour). The decisions below were relayed from the main session on 2026-11-09, none of them is new here. Where this section conflicts with the per-session requirements above, this section wins once the pack layer is built.

### User decisions (2026-11-09)

1. Move away from one secret gist per session.
2. Restic-style packs one level above the backend. The backend is a dumb object store, so it is not bound to gists and a self-hosted GitLab comes later.
3. One gist is one pack. The pack is logical, the gist store splits it into parts internally.
4. Pack size defaults to 64 MiB, configurable.
5. One open pack per machine, append-only, sealed when full. Prune runs only for dead data, never to clean up small packs.
6. Index per pack, readable without a clone.
7. Sharing (`agr share`) comes later, the design leaves room for it.
8. `agr sync delete` gets patterns (`*`, `?`) and `--all`, a collect-first preview and a second confirmation that asks the user to type the number when many sessions match. `--yes` skips only the first prompt, many sessions without a TTY need `--force`. Built after or with the pack layer, not before.
9. #166 is not merged to main before the pack layer is complete. The pack PRs target `feature/sync-gist`.
10. Progress and status apply to all networked sync subcommands. A compact default, `-v` for the full list, live lines and a loading line already exist and stay on the pack layer.
11. Auto push keeps the queue for parallel recordings, nothing is lost when several recordings finish at once.
12. Auto push stays off by default and the backend default is gist. Without `gh` or `git` nothing breaks.
13. Later, not now: rename the backend to show host and kind (for example `github-gist`), and a self-hosted GitLab backend.
14. The existing per-session gists (about 60 from the stopped push) are migrated or cleaned up safely, dry-run first, touching only gists we can decrypt.

### Stage 46 probe facts (user's live run, 2026-11-09)

- Appending from a partial clone pushes fine, but git fetched the existing part during the append. A second probe is pending to find a recipe that avoids it. This decides ADR Open risk 1 (branch P or branch C in the PLAN).
- `raw_url` plus Range work on secret gists for binary parts.
- A 700 KiB index comes back whole through the API.

### Scope (pack layer)

- Pack format, per-pack index, merge rules, the store trait, the gist mapping, append and seal.
- Auto push on packs, status and pull cost, progress output on all networked subcommands.
- `agr sync prune` for dead data, `agr sync migrate` for the old gists.
- `agr sync delete` with patterns, `--all`, preview and confirmation.
- `pack_size_mib` config key and the machine writer id.
- Docs: `docs/architecture/sync.md`, wiki command pages, README.

### Out of Scope (pack layer)

- `agr share` (the design leaves room, nothing is built).
- A GitLab or S3 backend and the rename of the backend to host and kind. The store trait only has to leave room.
- Dedup or content defined chunking, a local cache of sealed indexes, `raw_url` reads.
- Automatic prune, background prune, a remote lock.
- Any change to the key, the recovery phrase or blob v2.

### Requirements

**P1. No per-session gists**
- [ ] After the switch a push of N sessions creates pack gists only, never one gist per session and never a `.keep` stub.
- [ ] 440 MiB of sessions end up in about 7 sealed packs plus one open pack per machine (test on the memory store with sizes against the target).
- [ ] Every pack gist has the description `agr:v2` and nothing else readable.

**P2. Backend neutral store**
- [ ] The engine and pack layer run unchanged on a memory store and a directory store (one contract test passes on all stores).
- [ ] `layer_rules_hold` fails when `pack/` names `app`, `engine` or a concrete store, or when `store/` names `crypto`, `engine`, `pack` or `app`.
- [ ] The trait carries no gist, GitHub or part concept, only pack id, logical byte stream, index string and length.

**P3. One gist is one pack**
- [ ] The gist store maps one pack to one gist, data parts `data.NNN` plus one index file.
- [ ] Parts are cut at session boundaries at 8 MiB, a session over 8 MiB gets its own parts, existing parts are never rewritten.
- [ ] A pack that would pass 250 files is sealed early, the 300 file listing limit stays out of reach.

**P4. Pack size**
- [ ] `[sync] pack_size_mib` defaults to 64 and is read from the config, out of range values give a clear error (range 16 to 512 per ADR, see open question 1).
- [ ] A pack is sealed by the append that reaches the target, a session larger than the target gets its own pack and the open pack stays open.

**P5. One open pack per machine, append-only**
- [ ] A machine has at most one open pack and only appends to it, a sealed pack is never written again (a contract test shows a second append to a sealed pack is refused).
- [ ] An append writes new parts and the new index as one unit, a crash before the index leaves the old index valid.
- [ ] A wrong expected length gives `Moved` and writes nothing, one retry on top, then "changed on the remote, run again".
- [ ] Two writers on one writer id never overwrite each other (mandatory test).
- [ ] A lost local state does not lose remote data, the old open pack stays readable and merged.

**P6. Index per pack, readable without a clone**
- [ ] Each pack carries one sealed index with path, gen, sha256, size, offset, length, deleted and time per entry.
- [ ] `agr sync status` makes no `git clone`, only the listing plus one API call per pack.
- [ ] An index that would pass 768 KiB seals the pack early, a truncated index is reported as unreadable, never half read.
- [ ] A pack with another key or an unknown version is reported and never touched.

**P7. Merge and conflicts**
- [ ] The highest gen per path wins, a tombstone on top means absent.
- [ ] Two entries at the top gen with different hashes are a fork, reported per session, push and pull skip it, `push --overwrite` resolves it with gen + 1.
- [ ] Two entries at the top gen with the same hash are one session with two places, the reader tries the next on `NotFound`.
- [ ] A merged gen below the local last-sync gen is refused as before, the local file stays untouched.
- [ ] A fork a push has just created is reported in the same run, and status names the next step.

**P8. Prune only for dead data**
- [ ] `agr sync prune [--dry-run]` never touches an open pack and never a sealed pack under 20 percent dead bytes.
- [ ] A fully dead sealed pack is deleted, a pack at or over 20 percent is repacked with the same path, gen and sha256, tombstones included.
- [ ] An old pack gist is deleted only after the copies show up in a fresh index read.
- [ ] `--dry-run` prints bytes to move and bytes to free and changes nothing.
- [ ] Status prints a hint at over 20 percent dead bytes, no automatic prune exists.
- [ ] A pull that gets `NotFound` during a prune re-reads the indexes once and uses the copy.

**P9. Room for sharing**
- [ ] The pack reader takes its key as an argument and can read one pack without the merge, so a one-session share pack can reuse it later. `agr share` does not exist in the CLI.

**P10. `agr sync delete` with patterns**
- [ ] `agr sync delete` takes one or more patterns with `*` and `?`, or `--all`, matched against `<agent>/<name>`.
- [ ] It collects first and prints the full list with count and size before anything is asked or changed, `--dry-run` stops there.
- [ ] One match asks one confirmation. Many matches ask a second one where the user types the number of sessions, a wrong number aborts without changes (threshold in open question 2).
- [ ] `--yes` skips only the first prompt, never the typed number.
- [ ] Many matches without a TTY abort unless `--force` is given, one match without a TTY still needs `--yes` as today.
- [ ] A pattern without a match exits non-zero and says so, local files are never deleted.
- [ ] A delete is a tombstone in the open pack and the bytes are freed by the next prune (open question 3).
- [ ] It is built after or in the same PR as the pack layer, never on the per-session code.

**P11. Branch and merge order**
- [ ] #166 is not merged to main until the pack layer is complete.
- [ ] Every pack PR targets `feature/sync-gist`, each PR stays under 100 changed files.
- [ ] The probe file of Stage 46 stays an ignored test, the default `cargo test` stays offline.

**P12. Progress and status on every networked subcommand**
- [ ] `push`, `pull`, `status`, `delete`, `migrate` and `prune` print a compact summary by default and the full list with `-v`.
- [ ] Each shows the live line while it works and the loading line before the first answer, on a TTY only.
- [ ] The percent of a push belongs to the append, `NO_COLOR` is respected, no token appears in any output.

**P13. Auto push keeps the queue**
- [ ] N recordings that finish while a drain runs are all pushed, none lost and none pushed twice.
- [ ] One drain round is one append (one commit, one push), a burst during a drain lands in the next round together.
- [ ] A single recording is pushed without waiting for others (no local buffering).
- [ ] Pushes to one gist are paced 10 s apart.

**P14. Defaults and missing tools**
- [ ] Auto push is off by default, the backend default is `gist`, `off` disables sync.
- [ ] Without `gh` or `git` every sync command prints one clear message, exits non-zero and changes nothing, all other agr commands are unaffected.
- [ ] No token in config, state, log or output.

**P15. Migrate the per-session gists**
- [ ] `agr sync migrate` is a dry-run unless `--yes` and prints how many sessions move, are already moved, are empty stubs and are not ours.
- [ ] Only gists that decrypt with our key and `agr:pending` stubs older than 1 hour with exactly one `.keep` are touched, everything else is listed and kept.
- [ ] A moved session keeps path, gen and sha256, a hash mismatch keeps the old gist and is reported.
- [ ] An old gist is deleted only after the append holding its session returned and a fresh index read shows it, stubs are deleted directly.
- [ ] A rate limit stops with "N left, run again later", a re-run finishes, a listing past 3000 gists does not abort.
- [ ] The release that switches to packs ships `agr sync migrate`.

**P16. Docs**
- [ ] `docs/architecture/sync.md`, the wiki command pages, `pack_size_mib` in the configuration page and the README describe packs, forks, prune, migrate and delete patterns, and state what a third party with the URL can see.

**P17. Later (recorded, not built)**
- [ ] Nothing in the pack layer hardcodes the backend name `gist` outside the gist store, so a rename to host and kind and a GitLab store only add code.

### Open questions for the user

1. Pack size range: ADR accepts 16 to 512 MiB with default 64, is that the range you want?
2. Delete patterns: from how many matches do we ask the typed number, 2 or more, or a higher threshold like 10?
3. A delete is a tombstone and frees space only after `agr sync prune`, is that fine, or must it free space at once?
4. Pattern delete lands in which PR, with the switch (PR C), or its own PR after PR D?
