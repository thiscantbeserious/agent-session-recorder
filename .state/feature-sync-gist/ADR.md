# ADR: Encrypted Session Sync (secret gists first)

## Status
Accepted (auto-approved, the user delegated the SDLC up to an open PR). Variant A and D9 were decided by the user, not autonomously. Everything else was decided autonomously, review in PR.

## Context

Sessions live on one machine. REQUIREMENTS.md (auto-approved, updated 2026-11-06 with auto push,
encryption and key handling) fixes the what: `agr sync status|push|pull|delete`, a backend-neutral
`SyncBackend` trait, a hidden local index, three-way hash conflicts, secret gists via `gh` as the
only backend, every upload encrypted locally, the key in `~/.config/agr/sync.key` with a recovery
phrase for the second machine, and auto push after each recording once a backend is set.

This ADR decides the how: transport, crypto, remote layout, index, module layout, auto push
latency and the stage split.

Base: `main` after #165 (gzip storage). Config is v4, as such v5 is next. MSRV 1.88.

### Verified facts (2026-11-06)

**GitHub gists**
1. The REST API returns at most 1MB of content per file, `truncated: true` marks the rest. `raw_url`
   works up to 10MB, larger files need a clone via `git_pull_url`. A gist response lists at most
   300 files. Source: https://docs.github.com/en/rest/gists/gists (section "Truncation").
2. Gists are git repositories, they can be cloned, committed to and pushed like any repo.
   Source: https://docs.github.com/en/get-started/writing-on-github/editing-and-sharing-content-with-gists/forking-and-cloning-gists
3. Git warns on files over 50 MiB and GitHub blocks files over 100 MiB. Push size is capped at
   2GB, repos ideally stay under 1GB. Sources:
   https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-large-files-on-github
   and https://docs.github.com/en/repositories/creating-and-managing-repositories/repository-limits
4. Secret gists are not listed or searchable, but anyone with the URL can read them. GitHub also
   runs partner secret scanning on secret gists. Source:
   https://docs.github.com/en/get-started/writing-on-github/editing-and-sharing-content-with-gists/creating-gists
5. "List gists for the authenticated user" supports `per_page` and `since`, so `gh api gists
   --paginate` returns all of them with descriptions. Source:
   https://docs.github.com/en/rest/gists/gists#list-gists-for-the-authenticated-user

**gh CLI (local: gh 2.96.0, logged in, scopes `gist`, `read:org`, `repo`, git protocol ssh)**
1. `gh gist` has `create`, `list`, `view`, `clone`, `edit`, `rename`, `delete`. `create` is secret
   by default (`--public` to list), `delete --yes` skips the prompt, `clone` passes extra flags to
   `git clone` after `--`.
2. `gh gist create` and `gh gist edit` refuse binary content ("binary file not supported").
   Source: https://github.com/cli/cli/blob/trunk/pkg/cmd/gist/create/create.go (around line 214)
   and https://github.com/cli/cli/blob/trunk/pkg/cmd/gist/edit/edit.go (around lines 282, 421).
3. `gh auth status` prints the token scopes line but never the token. `gh api` authenticates with
   the stored token, agr never sees it.

**Crates (crates.io, checked 2026-11-06)**
- `chacha20poly1305` 0.10.1 (stable line, no declared MSRV, `getrandom` feature on by default),
  0.11.0 needs Rust 1.85. Both fit MSRV 1.88.
- `sha2` 0.10.9 (no declared MSRV), 0.11.1 needs exactly 1.88.
- `data-encoding` 2.11 (MSRV 1.48, no deps) covers base32 and base64url.
- `getrandom` 0.2 and 0.3 are already in Cargo.lock through other deps.
- Windows target `x86_64-pc-windows-msvc` is installed locally, so `cargo check --target` works.

**Codebase touch points**
- `src/recording.rs` (370 lines) `finish()` calls `finalize_recording` (src/recording/finalize.rs),
  which returns the final `.cast.gz` path. That is the auto push hook.
- `src/files/codec.rs` has `atomic_write`, `temp_path_for`, `is_gzip_path`, `session_stem`.
  `src/files/backup.rs` has `create_backup` / `backup_path_for`. `src/files/lock.rs` has
  `read_lock` and live-lock detection. `src/commands/prompt.rs` has `should_proceed` with the
  no-TTY rule. `src/config/state.rs` loads and atomically saves `state.toml`.
- `StorageManager::list_sessions` only walks directories under the storage root and skips
  hidden files, so a hidden file at the root (the index) is never a session and never in stats.
  `agent_dirs` does not skip hidden directories, as such temp clones must never live in the
  storage dir.
- `src/cli.rs` is 420 lines already, new subcommand enums go to `src/cli/subcommands.rs` (229).

### Still unknown (verify in a stage)
1. A git push of a binary file to a gist is accepted and served back byte-identical. Verify in
   Stage 7 (ignored live test). Fallback: base64url text in the same git file, about 33% larger,
   nothing else changes.
2. Maximum gist description length (the sealed metadata token is about 260 chars). Verify in
   Stage 7. Fallback: put the token into `agr-session.json` and read it per gist (one API call
   per gist on `list`, slower but correct).
3. With `git_protocol = https` and no gh credential helper, `git push` to the gist fails. Verify
   in Stage 7 and map the error to the hint `gh auth setup-git`.

## Options Considered

### 1. Transport for the blob

**Option A: REST API through `gh api` with base64 text files**
- Pros: one tool, no git, no temp clone.
- Cons: API reads truncate at 1MB per file and `raw_url` stops at 10MB, so a 52MB session needs
  chunking into 1MB files (about 70 files of base64, 33% larger), plus one JSON request body of
  70MB for create. Request body limits are undocumented.

**Option B: git clone and push of the gist repo, `gh` for create, list and delete**
- Pros: the blob is a real binary file, no size overhead, the documented path for files over
  10MB, single file per gist. `gh gist clone` reuses the user's git protocol setup.
- Cons: needs `git` on PATH, one temp clone per push or pull, the gist history keeps old
  ciphertexts (encrypted, acceptable).

**Option C: `gh gist create/edit` with files**
- Not possible, `gh` refuses binary, and ciphertext is binary.

**Decision: Option B**, decided autonomously (review in PR). It is the documented way for large
files and keeps one file per gist. Clones are shallow (`-- --depth 1`) into
`std::env::temp_dir()/agr-sync-<pid>-<n>`, removed afterwards, never under the storage dir.

### 2. Cipher

**Option A: XChaCha20-Poly1305 (`chacha20poly1305`)**
- Pros: 24-byte nonce makes random nonces safe for any number of messages, constant-time in
  software on every CPU, pure Rust, RustCrypto.
- Cons: none relevant here.

**Option B: AES-256-GCM (`aes-gcm`)**
- Pros: hardware accelerated on most CPUs.
- Cons: 12-byte nonce, random nonces need care past ~2^32 messages per key, software fallback
  is slower and needs care for side channels.

**Decision: Option A, `chacha20poly1305 = "0.10"`**, decided autonomously (review in PR). Speed
does not matter at sync sizes, nonce safety does. 0.10 over 0.11 because it shares the existing
`getrandom` 0.2 and is the long-standing stable line, bump later in a separate PR.

### 3. Recovery phrase encoding

**Option A: BIP39 word list**
- Pros: familiar words.
- Cons: a 2048-word list (crate or 13KB file), more than we need.

**Option B: base32 with checksum**
- Pros: `data-encoding` is tiny, has no deps and also gives us base64url for the metadata token.
  Case-insensitive, no ambiguous `0/O`, `1/I` with RFC 4648 base32.
- Cons: less pleasant to type than words.

**Decision: Option B**, decided autonomously (review in PR). Phrase = base32 (RFC 4648, no padding)
of `key (32 bytes) || first 2 bytes of SHA-256(key)`, which is 55 chars, printed as 11 groups of
5 joined with `-`, prefixed `agr1-`. Import upper-cases and strips spaces and dashes, rejects a
wrong prefix, wrong length or bad checksum.

### 4. Where the metadata lives and whether names are encrypted

**Option A: plain description `agr:v1 <agent>/<name>`**
- Pros: readable in the gist UI, one list call gives everything.
- Cons: session names come from directory and branch names (customer and ticket names), that is
  exactly what the user wants private.

**Option B: sealed metadata in the description, `agr:v1 <token>`**
- Pros: one paginated list call still gives path, plaintext hash and size for every gist, nothing
  readable leaks. Gists sealed with another key show up as "other key" and are left alone.
- Cons: the gist UI shows only an opaque token, description length to verify (fallback above).

**Decision: Option B**, decided autonomously (review in PR). `token = base64url(sealed JSON)`,
JSON is `{"v":1,"path":"claude/x.cast.gz","sha256":"<hex>","size":123}`. `.keep` in the gist
holds only a newline (gists need one text file at create time, `gh gist create` cannot create
the binary file), the blob lives in `data.000`, `data.001`, ... (see "Large sessions").
File names reveal neither the tool nor the content, only the `agr:v1 ` description prefix does
(second machines find their gists by it).

### 5. Auto push latency

**Option A: synchronous with a short timeout**
- Pros: the warning line is printed in the same terminal.
- Cons: create plus clone plus push is several seconds for small files, a 50MB upload is much
  longer, it blocks the shell after every recording, which the requirement forbids.

**Option B: fast local checks in-process, network part in a detached child**
- Pros: the shell returns right away. Every skip reason that is knowable locally (no backend,
  `auto_push = false`, no privacy ack, no key, plain or locked session, `gh` or `git` not on
  PATH) still prints its one-line hint synchronously.
- Cons: a network failure cannot print in the original terminal.

**Decision: Option B**, decided autonomously (review in PR). After the local checks pass, agr
spawns `agr sync push --auto <agent>/<name>` (hidden flag) with null stdio, its own process group
on unix, no wait. On failure the child stores one line in `state.toml`
(`last_auto_push_error`), and the next recording or `agr sync status` prints it as a warning and
clears it. The child takes the sync lock non-blocking and quietly skips if a push already runs,
the next push picks the session up. Exit status of the recording is never touched.

### 6. Hash

**Decision: `sha2 = "0.10"`**, SHA-256 over the `.cast.gz` bytes (plaintext of the cipher). The
index, conflicts and pull verification use this hash, never the ciphertext.

## Decision

### Data flow

```mermaid
flowchart LR
  cmd[agr sync commands] --> engine[sync engine]
  rec[recording finish] --> auto[auto push check]
  auto --> child[agr sync push --auto]
  child --> engine
  engine --> crypto[crypto seal/open]
  engine --> index[.agr-sync.json]
  engine --> trait[SyncBackend]
  trait --> gist[GistBackend gh + git]
  trait --> mem[MemoryBackend tests]
```
Legend: commands and auto push go through one engine, encryption sits above the trait, backends
only ever see opaque bytes and opaque metadata tokens.

### Trait

```rust
pub struct RemoteEntry { pub id: String, pub meta_token: String }

pub trait SyncBackend {
    fn name(&self) -> &'static str;
    fn check(&self) -> Result<(), BackendError>;            // gh/git present, logged in, scope
    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError>;
    fn push(&self, id: Option<&str>, meta_token: &str, blob: &[u8]) -> Result<String, BackendError>;
    fn pull(&self, id: &str) -> Result<Vec<u8>, BackendError>;
    fn delete(&self, id: &str) -> Result<(), BackendError>;
}
```
`BackendError` (thiserror) has `NotInstalled(&'static str)`, `NotLoggedIn`, `MissingScope`,
`Unreachable(String)`, `Other(String)`, so commands print one clear line per case. Encryption is
in the engine, as such a later S3 or git backend gets it for free.

### Blob and token format

`b"AGR1" || version u8 (1) || nonce [24] || ciphertext+tag`. AAD is `b"agr:blob:" || path` for
the blob and `b"agr:meta"` for the token, so a blob moved to another session's gist fails to
open.
**Superseded for the blob by D9 below** (streamed `AGR1 || 0x02`, no longer whole in memory), the
token keeps this one-shot format.

### Key file

`~/.config/agr/sync.key`, content `agr-sync-key v1 <base32 key>`, created with mode 0600
(`OpenOptions::mode`, unix only, `cfg(unix)` so Windows still compiles). Load refuses a file with
any group or other bits set. First creating push without a key generates one from `OsRng`, but
only if the remote has no agr gists, otherwise it aborts with the import hint (a second key
would split the remote into two unreadable halves). `agr sync key export` prints the phrase,
`agr sync key import [--force]` reads it from stdin (or the argument), refuses to replace a
different existing key without `--force`.

### Index

`<storage>/.agr-sync.json`, written with `codec::atomic_write`.
`{"version":1,"entries":{"claude/x.cast.gz":{"sha256":"..","remote_id":"..","synced_at":"<rfc3339>"}}}`.
Missing or unparseable is treated as empty (rebuild path, same rules as a second machine), with
one warning for unparseable. Never synced, never a session.

### Decision table (pure, `sync::plan`)

Inputs per path: local hash (or skip reason), remote hash and id, last synced hash.
1. local == remote: in sync (index refreshed when missing).
2. local only: push. remote only (also "in index but deleted locally"): pull.
3. local changed, remote == last: push. remote changed, local == last: pull.
4. both differ from last (or no last and local != remote): conflict, skipped unless
   `--overwrite`, which picks push or pull by command.
5. remote entry sealed with another key or unknown version: reported, never touched.
6. local plain `.cast`, live lock, over 1 GiB: skipped with reason.

### Safety rules
- Pull: download, open (auth), SHA-256 equals the sealed metadata hash, then
  `codec::atomic_write_raw` (new, same temp + fsync + rename as `atomic_write` but never
  encodes, because `atomic_write` gzips any `.cast.gz` target and would double-compress).
  An existing file is only replaced when the decision is pull (row 3, remote changed and local
  == last) or `--overwrite` on a conflict, after the current file is copied to the hidden
  `.bak`. An older, different `.bak` is rotated (replaced by the current file, the newer state
  worth keeping), it never blocks a pull.
- Push and pull take `<storage>/.agr-sync.lock` (create_new, pid inside, stale pid reclaimed).
- Index updated after each successful session, so an interrupted run loses nothing.
- Per-session errors go to the report, the run continues.
- No implicit deletes in either direction, `agr sync delete` only removes the gist and the
  index entry.
- `gh` is called with `std::process::Command`, never through a shell, args never contain the
  token, stderr is mapped to `BackendError` and never echoed raw into the index or state.

### Module layout

**Superseded** by "Module structure (refinement 2026-11-06)" below, the table shows the old flat files.

| File | Content |
|------|---------|
| `src/sync/mod.rs` | module list, `SessionPath`, re-exports |
| `src/sync/crypto.rs` | key type, seal/open, token encode, phrase encode/decode, sha256 helper |
| `src/sync/key.rs` | key file load/save, permissions |
| `src/sync/index.rs` | index load/save |
| `src/sync/plan.rs` | pure decision table |
| `src/sync/backend.rs` | trait, `RemoteEntry`, `BackendError` |
| `src/sync/memory.rs` | `MemoryBackend` (pub, used by tests) |
| `src/sync/scan.rs` | local scan (skip reasons) and remote decode into plan inputs |
| `src/sync/run.rs` | execute push, pull, delete, report, sync lock |
| `src/sync/push.rs` | push of one session, compare-and-swap, adopt and description repair |
| `src/sync/pace.rs` | clock and pacer for mutating GitHub calls |
| `src/sync/fail.rs` | why one push or pull did not finish (`Fail`) |
| `src/sync/record.rs` | index entry written after a push, refresh or pull |
| `src/sync/parts.rs` | sealed blob as `data.NNN` parts, rolling writer and chaining reader |
| `src/sync/blob.rs` | streamed blob v2, STREAM over XChaCha20-Poly1305 in 64 KiB frames |
| `src/sync/pull.rs` | pull of one session, verify, backup rotation, atomic write |
| `src/sync/lock.rs` | `.agr-sync.lock`, pid inside, stale pid reclaimed |
| `src/sync/format.rs` | plain text lines for status, preview, summary, delete |
| `src/sync/setup.rs` | backend and key resolution, error explanations, key import and export |
| `src/sync/file.rs` | directory backed fake, only through `AGR_SYNC_TEST_BACKEND_DIR` (prints one stderr line when set) |
| `src/sync/gist.rs` | `GistBackend` (`gh` and `git` calls) |
| `src/sync/gist_parse.rs` | list parsing, stderr mapping, scope check, pure |
| `src/sync/runner.rs` | `Runner` trait, `ProcessRunner` and `spawn_detached`, the only code that spawns processes |
| `src/sync/auto.rs` | auto push checks and error line |
| `src/recording/auto_push.rs` | recording hook, prints the hint and spawns the child |
| `src/commands/sync/` | `mod.rs` (dispatch, output), `key.rs` |

The library never prints, except the existing `State::load_from` warning. Commands format output
with theme helpers. `auto.rs` returns the hint line, `src/recording.rs` prints it.

### Config v5

**Update 2026-11-06**: auto-optimize (#184) took config v5 on main, as such the sync migration is now `migrations/v6.rs` with `CURRENT_VERSION = 6`. References to v5 below mean v6.

```toml
[sync]
# backend = "gist"   # unset means sync is off
auto_push = true
```
`SyncConfig { backend: Option<SyncBackendKind>, auto_push: bool }`, enum `Gist` with
`serde(rename_all = "lowercase")`, so an invalid value fails with the allowed values. v5 adds
the table with `auto_push = true` and the commented backend line, idempotent. `State` gets
`sync_privacy_ack: bool` and `last_auto_push_error: Option<String>`.

### Large sessions and listing limit (2026-11-06, user decision)

1. **Parts in the same gist**: the sealed blob is cut into parts of at most 45 MiB, named
   `data.000`, `data.001`, ..., all in the session's own gist and pushed in one commit with the
   rest. 45 MiB stays under git's 50 MiB warning. A re-push removes stale higher parts in the
   same commit. Pull reads every `data.NNN`, requires them contiguous from 000 (no gap, no
   extra, no reorder) and joins them before anything is opened. Split and join are pure
   (`sync::parts`), the engine and `SyncBackend` still move one blob.
2. **1 GiB cap** per session (was 100MB), from GitHub's repository size guidance
   (https://docs.github.com/en/repositories/creating-and-managing-repositories/repository-limits).
   The cap is checked on the local `.cast.gz` size (D10 follow-up). The memory bound this point
   described is gone with D9, memory is flat now.
3. **Listing abort at 3000**: GitHub documents a 3000 gist pagination cap
   (https://docs.github.com/en/rest/gists/gists). If the listing has 3000 or more gists in
   total (all gists, not only agr ones) the backend errors and the engine aborts before any
   change, so unlisted gists are never treated as missing.

### Variant A: manifest, generation and compare-and-swap (2026-11-06, accepted, built in Stages 14 to 31)

**Status**: Accepted (user decisions D1 to D5 on 2026-11-06), built in Stages 14 to 31 (D9 streaming is Stages 25 to 31). The points
marked **USER DECISION** below are settled by D1 to D5, D6 to D8 came after the batch 12 review.

**Problem**: push today is clone, commit, `git push`, then a separate description PATCH
(`src/sync/gist.rs` `upload`), and the engine never checks that the remote is still the state
it planned against (`src/sync/run.rs` `push_one`). Two machines pushing the same session can
lose an update, an interrupted create leaves an orphan, a rollback through gist history is not
detected, 403/429 are a generic `Other` and the run keeps going, and commit dates carry the
local timezone.

#### Rejected: HMAC and ciphertext hash in the file names

The earlier proposal named the gist files after an HMAC of the path plus the ciphertext hash.
**Rejected**, because:
1. A git commit is already atomic, the parts and their metadata land together or not at all,
   file names add nothing to that.
2. A stable HMAC of the path is the same on every push, as such it links gists and revisions
   to one session for anyone who can read the gist list (linkability leak), and it needs a
   second key derived from the sync key (key separation) that we do not have today.
3. It does not fix the real races (plan against a stale listing, two pushes in the same
   seconds, the separate description PATCH).

#### Decision

1. **Manifest file**: every session gist holds a file `manifest` in the same commit as its
   `data.NNN` parts. Its content is the sealed metadata token, the same string that goes into
   the description, `Meta { v: 2, path, sha256, size, gen }`. The commit is the truth, the
   description is a listing hint, and repairing the description is copying the manifest token
   into it.
2. **Generation**: `gen` starts at 1 and is planned gen + 1 on every push. Because every push
   is a compare-and-swap (point 3) two machines can never both commit the same gen on one gist.
3. **Compare-and-swap push**: the engine passes the token it planned against (`expected`,
   `None` for a new or pending gist). The gist backend clones, reads `manifest` (missing means
   `None`), and when it is not byte-equal to `expected` it returns `BackendError::Changed` with
   the current token and writes nothing. Otherwise it writes the parts and the new manifest,
   commits and runs a plain `git push`. Never `--force`, never a `+` refspec, never a pull or
   rebase retry. A non-fast-forward rejection maps to `Changed(None)`.
4. **Blob AAD**: `b"AGR1" || 0x01 || b"agr:blob:" || path || ":" || gen`, format tag in AAD
   (domain separation, wire header checked separately) and gen bound, the core `seal` and `open` (and the KAT vector) stay untouched.
4a. **Clock and pacer** (Batch 10 review): one `pace::Clock` trait (`mono`, `unix_secs`,
   `sleep`, all `&self`) behind `Arc<dyn Clock>`, `SystemClock` in production and a mutex
   backed fake in tests. `unix_secs` errors on a pre-epoch clock instead of giving 1970.
   `GistBackend` holds the clock for commit dates and a `Mutex<Pacer>` built from the same
   clock, so `&self` trait methods can pace and no generic leaks into the public type. The
   per-call `wait` wiring stays in Stage 18.
5. **Push order**: `create` (pending gist) then the index entry with the id and
   `pending = true`, then the compare-and-swap push, then the index entry with token and gen,
   then the description PATCH. A failing step keeps the gist, nothing is deleted on error any
   more, the next run resumes it through the index id.
6. **Pull**: the backend returns the manifest token and the joined blob from the same clone.
   The engine opens the manifest, requires the row path, and rejects it when its gen is lower
   than the index gen or lower than the planned gen (rollback or stale copy, local file
   untouched). The blob is opened with the manifest gen and checked against the manifest
   SHA-256. A manifest newer than the description is accepted (manifest wins), the pull was
   only planned because the local file equals the last synced state, as such nothing local is
   lost.
7. **Description repair**: the description is only ever rewritten to the newest known token,
   that is the index token when the index gen is higher than the description gen, or a
   `Changed` token whose gen is at least the index gen and the description gen. It never moves
   to a lower gen.
8. **Adopt own commit**: `Changed(Some(t))` where `t` opens and its SHA-256 equals the local
   file is our own earlier push (crash after the commit, before the index save), the engine
   records it and repairs the description instead of reporting a conflict.
9. **Index id fallback**: when the listing misses a path but the index has a remote id, the
   push goes to that id with the index token as `expected`. `BackendError::NotFound` (gist
   deleted on GitHub) is reported as "remote copy gone" and skipped (D6), no automatic
   recreate. Only `push --overwrite` creates a new gist and updates the index. An index id
   that is listed as another key or an unknown version is never pushed into.
10. **Duplicates**: `RemoteEntry` gets the server `created_at`. Several gists for one path
    resolve to the oldest `created_at`, id as tie-break, so every machine picks the same one.
    Same SHA-256 means the others are reported with a hint, a different SHA-256 makes the path
    `Duplicate`, skipped and reported with both ids, also with `--overwrite`.
11. **Rate limits**: 403 or 429 with `rate limit` in the text, or `secondary rate limit`, map
    to `BackendError::RateLimited`. The engine stops the run at the first one, keeps what is
    done (index is saved per session) and the report says "N left, run again later". Every
    mutating call (create, `git push`, PATCH, delete) is paced to at most one per second.
12. **UTC dates**: `git commit` runs with `GIT_AUTHOR_DATE` and `GIT_COMMITTER_DATE` set to
    `@<unix seconds> +0000`. `Runner::run` gets an `env` argument for exactly that, the process
    env of the tests is never touched.
13. **Compatibility**: nothing is released. A gist with a v1 token (no manifest) opens as
    `v != 2` and becomes the existing `UnknownVersion` row, reported and never touched, the
    maintainer deletes the test gists by hand. **USER DECISION** D5, the task allowed "treated
    as needing a push", this is the zero code option.

#### Trait (changes, and why)

```rust
pub struct RemoteEntry { pub id: String, pub meta_token: String, pub created_at: String }

pub trait SyncBackend {
    fn name(&self) -> &'static str;
    fn check(&self) -> Result<(), BackendError>;
    fn list(&self) -> Result<Vec<RemoteEntry>, BackendError>;
    fn create(&self) -> Result<String, BackendError>;                  // pending, unlisted
    fn push(&self, id: &str, expected: Option<&str>, manifest: &str, blob: &[u8])
        -> Result<(), BackendError>;                                    // compare-and-swap
    fn describe(&self, id: &str, token: &str) -> Result<(), BackendError>;
    fn pull(&self, id: &str) -> Result<(Option<String>, Vec<u8>), BackendError>;
    fn delete(&self, id: &str) -> Result<(), BackendError>;
}
```
`BackendError` gains `Changed(Option<String>)`, `NotFound`, `RateLimited`.
Why: `create` is split out so the engine stores the id before any upload (no orphans),
`expected` is the compare-and-swap, `describe` is split out because repair and the crash
window need it on its own, `pull` returns the manifest so it is checked from the same commit
as the blob. The backend still only sees opaque tokens and bytes, the comparison is plain
string equality.

```mermaid
sequenceDiagram
  participant E as engine
  participant I as index
  participant B as backend
  E->>B: create (new only)
  E->>I: id, pending
  E->>B: push(id, expected, manifest, blob)
  B-->>E: ok or Changed
  E->>I: token, gen
  E->>B: describe(id, token)
```
Legend: one push of one session, every arrow after `create` is safe to repeat on the next run.

Index entry: `{sha256, remote_id, synced_at, gen, token, pending}`, the three new fields with
`serde(default)`, index version stays 1 (nothing released).

#### Design review (architect)

1. **Is the compare-and-swap race free?** There are three windows.
   - Plan to clone (listing at t0, clone at t1): closed by the manifest equality check in the
     clone.
   - Clone to push (two pushes within seconds): closed by git itself, the server updates the
     ref only when our parent is still the tip, the second push gets non-fast-forward and maps
     to `Changed`. This holds only while we never force and never auto retry, both get a test
     on the argument list.
   - Commit to description PATCH: **not closed**, the gist API has no conditional PATCH. A
     stalled PATCH can land after a newer push and its PATCH, the description then shows an
     older gen than the manifest. Nothing is lost (every push and pull checks the manifest in
     the clone), it only hides the newer version from listings until it heals. It heals when
     the newest writer runs again (index gen higher than description gen, point 7) or when any
     machine pushes that session (`Changed` with a newer token, point 7). Documented as a
     known limit, the window is the length of one stalled HTTP call.
2. **Is `gen` safe with concurrent writers?** Yes on the gist, two machines planning N+1 from
   N both clone gen N, one push wins, the other is non-fast-forward. It is only safe because of
   point 3, a backend without a ref compare-and-swap would need another mechanism, the memory
   backend models it with one mutex. A lost index on a machine means trust on first use, the
   fresh index accepts whatever gen the remote has, so a rollback done before that machine
   syncs again is not detected there. Documented as a known limit, the other machines still
   detect it.
3. **Does gen in the blob AAD create a recovery problem?** No, manifest and parts are in one
   commit, they cannot get out of step through a partial failure. Honest note: it is
   redundant, the authenticated manifest SHA-256 already rejects an old blob with a new
   manifest. It costs one line, as such we keep it as the requirement asks.
4. **Is "manifest wins, description repaired" safe with a rolled back manifest?** Only with
   the gen rule of point 7, a manifest with a lower gen than the index or the description is
   rejected and never copied into the description. Without an index (fresh machine) a rolled
   back manifest wins, same trust on first use limit as above.
5. **Can dedupe delete the only good copy?** Yes, the duplicate that is not the oldest can hold
   the newer content, and deleting after verifying the kept gist does not change that. Report
   only is simpler and loses nothing, duplicates are rare once the index id fallback and the
   pending resume exist. **USER DECISION** D1: drop the automatic delete, keep the
   deterministic pick by `created_at` and id, report duplicates with both ids. Recommended.
6. **Throttle numbers**: GitHub documents at most 80 content-generating requests per minute
   and 500 per hour (https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api),
   whether gist git pushes count is not documented. A new session costs up to three mutating
   calls (create, `git push`, PATCH), an existing one two. Pacing every mutating call to one
   per second gives at most 60 per minute even if git pushes count, which leaves 20 for other
   `gh` use, that is at most 20 to 30 sessions per minute. The hourly 500 is reached after about
   eight minutes of a large first push, as such a big first sync will stop with "N left" and
   needs two or three runs. **USER DECISION** D3 (decided 2026-11-06): pace only, no own
   hourly stop. agr spaces mutating calls one second apart and stops the run cleanly as soon as
   GitHub reports a rate limit, it never retries automatically after that (the architect had
   recommended an extra stop after 450 calls, the user chose pace only). Auto push moves one session per recording, it never runs into the hourly
   limit by itself, when it is rate limited the error line is stored and shown after the next
   recording, the next recording tries again with the same one or two calls.
7. **retry-after**: `gh gist create` and `git push` never show response headers, only `gh api`
   with `--include` would, and that output is not verified. **USER DECISION** D4: honour
   retry-after by never retrying automatically, the stop message says "run again later", no
   header parsing. Recommended.
8. **`parts` in the manifest**: the engine seals the manifest before the backend splits the
   blob, it does not know the part size, and a missing or extra part already fails the
   contiguity check and the AEAD tag. **USER DECISION** D2: drop `parts` from the manifest.
   Recommended.
9. **What else is over-engineered for one user with 2 to 3 machines?** Dropped already: file
   name HMAC, automatic dedupe delete (D1), header parsing (D4), `parts` (D2), a separate
   manifest AAD (the token AAD is enough), an index version bump. Kept because they are cheap
   and close a real loss: compare-and-swap, pending resume, index id fallback, gen check,
   description repair, `created_at` pick, rate limit stop, pacing, UTC dates.

#### Known limits (documented, not fixed)

- GitHub lists at most 3000 gists, sync aborts at that size.
- Session count, sizes and push times are visible to anyone who can see the gists.
- A rename leaves the old name on other machines until `agr sync delete`, and
  `agr sync delete` racing a push elsewhere is not a compare-and-swap, the other machine
  re-creates the gist from its local copy on its next push.
- A leaked key exposes every old revision in the gist history.
- The description can lag behind the manifest after a stalled PATCH, until it heals (review 1).
- Rollback detection is trust on first use for a machine without an index (review 2).

### D9: streaming encryption (2026-11-06, accepted, built in Stages 25 to 31, Stage 25 is B1 and B2)

**Status**: Accepted. The user asked for it (D9) and accepted the design on 2026-11-06, see Decision
History.

**Problem**: push reads the whole session and seals it in one AEAD call (`src/sync/push.rs`
`push_one`, `src/sync/crypto.rs` `seal`), the gist backend splits a `&[u8]` and joins every part
into one `Vec<u8>` on pull (`src/sync/gist.rs` `stage_parts`, `pull`), pull opens the whole blob
in memory and scan hashes the whole file in memory (`src/sync/scan.rs` `local_state`). A 1 GiB
session needs about 2 GiB of RAM on push and on pull. Memory has to stay flat up to the cap,
the format has to detect truncation, reordering, dropped and duplicated chunks, and pull must
write nothing unless the whole stream verified.

#### Options

1. **STREAM from the already used RustCrypto `aead` crate** (chosen). `chacha20poly1305 0.10.1`
   and `aead 0.5.2` are in `Cargo.lock`. `aead` has a `stream` module (STREAM, Hoang,
   Reyhanitabar, Rogaway, Vizar 2015, https://eprint.iacr.org/2015/189.pdf) behind the
   `stream` feature, `chacha20poly1305` forwards it as `stream = ["aead/stream"]`, the feature
   has no extra dependencies and `aead` declares `rust-version = "1.56"`, as such MSRV 1.88 is
   fine and no new crate comes in. Checked in the vendored sources
   (`~/.cargo/registry/src/*/aead-0.5.2/src/stream.rs`, `Cargo.toml` of both crates), docs.rs
   page: https://docs.rs/aead/0.5.2/aead/stream/index.html. `StreamBE32` builds the nonce as
   `prefix || counter u32 BE || last flag u8`, with XChaCha20-Poly1305 the prefix is 19 bytes.
   `EncryptorBE32::encrypt_next_in_place` / `encrypt_last_in_place` take the AAD per chunk.
   The same construction is used by age (https://github.com/C2SP/C2SP/blob/main/age.md,
   "payload", 64 KiB chunks, counter and last flag in the nonce) and libsodium secretstream
   is a close relative (https://doc.libsodium.org/secret-key_cryptography/secretstream).
2. **Own chunking with a per-chunk index in the AAD and a final length record**. Same idea,
   more code we own, no proof. Rejected.
3. **libsodium secretstream through a binding**. New C dependency, rekeying and tags we do not
   need. Rejected (constraint: no new heavy dependencies).

#### Blob format v2

```
AGR1 (4) || 0x02 (1) || nonce prefix (19, random) || frame 0 || frame 1 || ... || frame n
frame i < n : ciphertext of exactly 65536 plaintext bytes + tag (16) = 65552 bytes, last flag 0
frame n     : ciphertext of 0..=65536 plaintext bytes + tag (16), last flag 1
AAD (every chunk) = AGR1 || 0x02 || "agr:blob:" || path || ":" || gen
```

- Chunk size 64 KiB is a constant of version 2, not stored in the header, a different size
  is a new version byte. Overhead is 16 bytes per 64 KiB (0.025 %), a 1 GiB session is 16384
  chunks, far below the 2^32 counter.
- An empty session is one empty last frame (16 bytes). A session that is an exact multiple
  of 64 KiB ends with a full frame that carries the last flag, there is never an empty
  trailing frame after a full one.
- Magic and version are authenticated through the AAD, the nonce prefix through the nonce
  itself (a changed prefix gives a different nonce and every tag fails).
- The manifest token stays the one-shot `AGR1 || 0x01` format with `agr:meta` AAD, it is
  small. Only the blob moves to v2. `crypto::open` on a v2 blob and the v2 opener on a v1
  blob both fail with `UnsupportedVersion`.

Reading frames: wrap the input in a `BufReader`, read up to one frame with
`take(FRAME).read_to_end`, then `fill_buf().is_empty()` tells whether more data follows. More
data means `decrypt_next`, none means `decrypt_last`. Sealing uses the same look-ahead on the
plaintext to decide `encrypt_next` or `encrypt_last`.

| Attack on the stored ciphertext | Why it fails |
|---|---|
| Truncation at a frame boundary | the new last frame was sealed with flag 0, it is opened with flag 1 |
| Truncation inside a frame | short frame is opened as last, tag fails, under 16 bytes is `Truncated` |
| Reorder or swap frames | the counter is part of the nonce, frame i only opens at position i |
| Drop a frame | every later frame sits one position off, dropping the last leaves a flag 0 frame at the end |
| Duplicate a frame | same as reorder, the copy sits at the wrong counter |
| Append bytes or a frame after the end | the real last frame is then opened with flag 0 and fails |
| Frame from another blob, another gen or another session | other nonce prefix, other gen or path in the AAD |
| Old v1 blob replayed | version byte check, then AAD carries `0x02` |

#### Parts and chunks stay independent

The 45 MiB gist parts are a plain byte split of the sealed stream, a part boundary can fall
inside a frame. Integrity comes from STREAM over the joined bytes, `check_sequence` keeps
guarding the part names. Aligning parts to frames would couple two layers for no gain (a part
size of 45 MiB is not a multiple of 65552 anyway). `parts::split` and `parts::join` go away,
`part_name`, `check_sequence` and `stale` stay.

#### Trait and data flow

```rust
/// Writes the sealed blob into `out`, may be called again for a retry.
pub type BlobSource<'a> = &'a dyn Fn(&mut dyn std::io::Write) -> std::io::Result<()>;

fn push(&self, id: &str, expected: Option<&str>, manifest: &str, blob: BlobSource<'_>)
    -> Result<(), BackendError>;
/// Manifest and a reader over the stored blob, read from the same clone.
fn pull(&self, id: &str) -> Result<(Option<String>, Box<dyn std::io::Read>), BackendError>;
```

- Push gives a callback, not a reader, as `Push::run` can send twice (`Gone` plus
  `--overwrite`) and each call has to re-read the file. The callback opens the session file
  and runs the sealer straight into `out`, a fresh nonce prefix each time.
- The gist backend runs the callback only after the compare-and-swap check, into a small
  `PartWriter` (Write impl that rolls `data.NNN` files at `part_size`, creates `data.000`
  only on finish for an empty blob), then drops stale parts and writes the manifest as today. A callback error aborts
  before `git add`, nothing is committed, the scratch dir is removed on drop.
- The gist `pull` returns a reader that owns the `Scratch` and chains the part files in
  order (opened after `check_sequence`), the clone is removed when the reader is dropped.
- File backend: `File::create` plus callback on push, `File::open` on pull. Memory backend
  (tests only): callback into a `Vec`, `Cursor` on pull. Byte helpers `put`, `set_blob`,
  `blob` stay as they are for tests.
- `crypto::sha256_reader(impl Read) -> io::Result<(String, u64)>` hashes by `io::copy` into
  `Sha256` (sha2 implements `Write` with its default `std` feature). Scan uses it on the file
  instead of `fs::read`.

```mermaid
flowchart LR
  F[session file] --> S[sealer 64 KiB] --> P[PartWriter] --> C[clone data.NNN] --> G[git push]
  C2[clone data.NNN] --> R[chained reader] --> O[opener 64 KiB] --> T[hidden .tmp] --> V{hash ok} --> B[.bak] --> N[rename]
```
Top row is push, bottom row is pull, every arrow carries at most one frame at a time.

**Push**: `push_one` hashes the file again in a streamed pre-pass (hash and size, the manifest
meta is built from these, not from the scan). The sealer returns the plaintext SHA-256 and size
it saw, and `seal_file` compares them with that pre-hash and fails with "session changed while
pushing, run again" when they differ. So a file that changed between scan and push uploads the
new content under a manifest for the new content (the compare-and-swap still holds), and a
file that changed between the pre-pass and the seal is never committed.

**Pull safety**: new `codec::atomic_write_raw_with(path, fill)` (same hidden temp, fsync,
rename and temp removal on error as `atomic_write_raw`, the `commit` helper already does the
removal). The fill closure, in this order: open the stream into the temp writer (every frame
and the last flag verified), compare the returned SHA-256 with the manifest, then
`backup_existing`. Only then does `commit` rename. Any failure (tag, truncation, hash, backup,
disk) returns an error, the temp is removed, the local file and the `.bak` stay untouched.
Unverified plaintext only ever sits in the hidden temp file next to the target (hidden-file
rule from #160, scan skips it). A crash in the middle leaves the hidden temp, the next pull
of that session truncates and reuses it.

#### Memory bound

Data path per direction: one plaintext chunk and one frame buffer (2 x 64 KiB), a `BufReader`
(8 KiB default) and the hasher, under 256 KiB, independent of the session size. Peak RSS of the
process is then the binary, the gist listing and the index, a few MiB to a few tens of MiB.
Not counted: `git` child processes, git loads one blob at a time below `core.bigFileThreshold`
(512 MiB), our parts are 45 MiB.

**Perf test** (`tests/performance/sync_memory_test.rs`, `#[ignore]`, run with
`cargo test --release --test performance -- --ignored sync_memory --nocapture`): same pattern as
`gzip_memory_test.rs`. Builds a gzip of random bytes (`Compression::none()`, fast) at 64 MiB
and at 512 MiB, runs the release binary under `/usr/bin/time` (`-l` macOS, `-v` Linux) with
`AGR_SYNC_TEST_BACKEND_DIR` (file backend, no network, no git) for `agr sync push --yes` on
machine A and `agr sync pull --yes` on machine B after `key import`. Prints all four peak RSS
values and asserts the 512 MiB run is at most 16 MiB above the 64 MiB run for push and for
pull, and every run stays under 64 MiB. Before D9 the same test shows roughly 2x the session.

#### Migration

Blob v1 (`AGR1 || 0x01` one-shot) was never released, nothing to migrate, same reasoning as D5.
A v1 blob on a test gist fails to pull with "unsupported blob version 1", push replaces it on
the next change or with `--overwrite`.

#### Also in this round (same files)

- **B1** (`push.rs` `adopt`): on `Changed(Some(theirs))` that is not our content, when `theirs`
  opens with our path and its gen is at least max(index gen, row gen), describe the gist with
  `theirs` (best effort, only `RateLimited` stops the run), then report CHANGED. So the
  listing heals for the loser of a race and the next run plans against the real remote.
- **B1** (`scan.rs` `row`): a pending index entry (sha256 `""`) is never `last`, it is
  filtered out like its token already is.
- **B2** (`pull.rs` `pull_one`): `engine.backend.pull(id).map_err(Fail::from)?`, a rate limit
  during pull stops the run, `NotFound` stays `Gone`.

#### Challenge

1. **What can go wrong?**
   - Session changes between scan and seal: caught by the hash check in `seal_file`, the
     push fails for that session and nothing is committed.
   - The look-ahead is the one subtle bit: a decoder that calls `decrypt_last` on a full frame
     without checking for more data would accept appended frames. Tests cover truncation at
     every boundary, append, drop, swap and duplicate on a 3 to 4 frame blob.
   - Backup inside the fill closure means the `.bak` is written while the hidden temp exists,
     a failed backup removes the temp, nothing renamed. Ordering is tested.
   - The reader returned by gist `pull` holds the scratch clone, an engine that keeps it
     around keeps 1 GiB on disk longer. `pull_one` (`write_session` and `verify_into`) drops it at the end of the function.
   - Disk use is unchanged in total (clone plus temp), only RAM drops.
2. **What is over-engineered?** Dropped: chunk size or any parameter in the header (version
   byte covers it), aligning parts to frames, own STREAM code, a streaming memory backend,
   rekeying. Kept the plaintext SHA-256 although STREAM already authenticates, it is the sync
   identity for the three-way compare and it catches a file that changed during push.
   `seal_blob` / `open_blob` byte wrappers stay for tests only (thin, over a `Cursor`).
3. **Code size**: the stream code goes into a new `src/sync/blob.rs` (crypto.rs is already
   459 lines), about 150 lines with tests.

## Consequences

- New deps: `chacha20poly1305`, `sha2`, `data-encoding` (all pure Rust, small).
- New runtime requirement for sync only: `gh` and `git` on PATH.
- Gist UI shows only `agr:v1 <token>` and opaque `data.NNN` files, which is the point.
- Losing every copy of the key makes the remote unreadable. Docs and the first key generation
  say so.
- Help and completions snapshots change, docs regenerated with `cargo xtask gen-docs`.
- Expected PR size about 45 files, under the 100 file limit. Variant A adds about 3 files
  (`src/sync/push.rs`, `src/sync/pace.rs`, `tests/integration/sync_multi_machine_test.rs`),
  the branch is at 77 today.

## To verify during implementation
1. Binary git push to a gist round-trips (Stage 7, ignored live test).
2. Description length fits the token (Stage 7).
3. https protocol without credential helper error mapping (Stage 7).
4. `cargo check --target x86_64-pc-windows-msvc` passes with the new deps (Stage 1 and Stage 11).

**Variant A, still unverified live (Stage 18, fake runner only):** the compare-and-swap read of
`manifest` after `gh gist clone`, the plain `git push` rejection texts mapped to `Changed(None)`
(`! [rejected]`, `non-fast-forward`, `fetch first`), the `created_at` field in the list call, the
404 mapping to `NotFound`, and the rate limit texts mapped to `RateLimited`. The ignored live test
in `tests/sync_gist_live.rs` covers them and was not run.

**Still unverified live (Stage 7 was tested against a fake process runner only, no real gist was
created, edited or deleted, the ignored `tests/sync_gist_live.rs` was not run):**
- Exact `gh gist create <file> --desc <text>` output format (the id is parsed from the last path
  segment of the printed `gist.github.com/...` URL).
- `gh gist clone <id> <dir> -- --depth 1` works for a secret gist and with the configured git
  protocol.
- A binary `data.000` pushed with plain `git push` is served back byte-identical (Open
  Question 1), gist description length for the token (Open Question 2), and the real https
  without credential helper error text (Open Question 3, the mapping is built from sample
  strings, not captured output).
- `gh api gists?per_page=100 --paginate` prints concatenated JSON arrays (parser accepts that and
  a single array), and `gh api -X PATCH gists/<id> -f description=...` updates the description.
- The `gh auth status` scope line wording (`Token scopes: ...`) on other gh versions.
- Not verified live: `gh gist create` accepting the empty dotfile `.keep` with a newline body, a
  multi-part push and shrink (`tests/sync_gist_live.rs`, run by hand).
- Push order, not verified live (replaces the old order that deleted a newly created gist on
  error): a new gist is created with the placeholder description `agr:pending` (no `agr:v1 `
  prefix, so `list` ignores it), then clone, compare the manifest with the expected one, one
  commit with manifest and parts, plain `git push`, and only then `describe(token)`. A failed
  push never deletes a gist, an interrupted create stays in the index as pending and the next
  push resumes it. A crash before the description leaves an unlisted pending gist or an old
  description with a new manifest, pull trusts the manifest and the description is repaired
  later.
- Measured peak RSS of the D9 perf test (`tests/performance/sync_memory_test.rs`) is recorded in
  the PLAN progress notes of Stage 30.

## Decision History
1. 2026-11-06: user added auto push after recording (default on once a backend is set).
2. 2026-11-06: user required private storage, gists have no private mode, decided: keep secret
   gists, encrypt every upload locally, key file plus recovery phrase. GitHub secrets rejected
   by the user (write-only for `gh`).
3. 2026-11-06 (fix round): decision 9, a pull replaces an older different `.bak` with the
   current local file (rotate), a stale backup must not fail every later pull.
4. 2026-11-06 (fix round): decision 10, push order is blob first and token last with a
   placeholder description at create, so an orphan gist never advertises a hash without a blob.
5. 2026-11-06 (after adversarial review): user chose Variant A (manifest, gen, compare-and-swap),
   the HMAC file name proposal is rejected, D1 to D5 decided by the user on 2026-11-06: D1 report only, D2 drop
   `parts`, D3 pace only, D4 no retry-after parsing, D5 v1 gists left alone.
6. All other decisions in this ADR: decided autonomously (review in PR).
- 2026-11-06: Variant A decisions by user: D1 duplicates report only, no auto delete; D2 no `parts` in the manifest; D3 pace only (60 mutating calls per minute), no own hourly stop, stop when GitHub reports a rate limit; D4 no retry-after parsing, "run again later"; D5 unreleased v1 gists shown as unknown and left alone.
- 2026-11-06: Batch 12 review decisions by user: D6 a deleted gist (gh API 404) is reported as "remote copy gone" and skipped, only `push --overwrite` recreates it (replaces the automatic recreate in ADR item 9); git "Repository not found" is never NotFound (auth/access failures must not cause duplicates). D7 missing created_at fails closed in the duplicate pick. D8 a duplicate pick may change once after a pending gist gets its description, reported, nothing deleted.
- 2026-11-06: PO acceptance follow-ups by user: D9 stream the encryption in chunks so memory stays flat up to the 1 GiB cap (format change, gets its own design section and review); D10 keep `AGR_SYNC_TEST_BACKEND_DIR` in release builds, documented as test-only.
- 2026-11-06: D9 design by the architect (autonomous, review in PR): STREAM (`aead` `stream` feature, `StreamBE32`, XChaCha20-Poly1305), 64 KiB chunks, blob v2 `AGR1 || 0x02 || prefix19`, parts and chunks independent, push callback and pull reader in the trait, verify into hidden temp then backup then rename, v1 not migrated (unreleased). B1 and B2 from the Variant A review go into the same round.
- 2026-11-06: D9 design accepted by user: STREAM over XChaCha20-Poly1305 via `aead::stream`, 64 KiB chunks, plaintext SHA-256 check kept as sync identity.
- 2026-11-06: D11 by user (D9 batch 1 review): the B1 description repair re-reads the gist manifest right before describing and only describes if it still equals the token it adopts. GitHub has no conditional description update, so a millisecond window remains in which a concurrent newer describe can be overwritten; documented as a known limit, it self-heals when the newer machine runs again.
- 2026-11-06: D9 measured (Stage 30, release build, macOS, file backend): peak RSS push 64 MiB 7.7 MiB, push 512 MiB 7.7 MiB, pull 64 MiB 7.6 MiB, pull 512 MiB 7.5 MiB. Memory does not grow with session size.

## Module structure (refinement 2026-11-06)

**Status**: Accepted (approved by the user on 2026-11-06).

**Problem**: `src/sync/` is 24 flat files (about 6.6k lines), almost every module is `pub mod`, five
files are over 400 lines (`gist.rs` 1028, `blob.rs` 551, `format.rs` 511, `crypto.rs` 434,
`auto.rs` 429) and the layering only exists in our heads. Commands, recording and tests reach
into any module they like, as such every internal rename is a public API change.

**Approach**: four layers as folders, a small facade in `sync/mod.rs`, one hidden `testing`
module for the integration tests, and a unit test that fails when a layer imports what it must
not. Pure moves first, then splits, then visibility.

### Target tree

```
src/sync/
  mod.rs            facade: layer declarations, public re-exports, `testing`, layer check test
  crypto/           depends on nothing in sync
    mod.rs          Key, CryptoError, sha256_hex, sha256_reader
    seal.rs         one-shot `AGR1 || 0x01` seal and open, metadata token (Meta, seal_meta, open_meta)
    phrase.rs       recovery phrase export and import, checksum, PhraseError
    blob/
      mod.rs        streamed blob v2, STREAM over XChaCha20-Poly1305, 64 KiB frames (D9)
      tests.rs      cfg(test): truncation, reorder, bit flip, nonce layout and known-answer tests
  backend/          opaque bytes and tokens only, never crypto
    mod.rs          SyncBackend trait, RemoteEntry, BackendError, BlobSource, push_bytes, pull_bytes
    memory.rs       MemoryBackend, in-memory fake with failure hooks
    file.rs         FileBackend, directory fake behind AGR_SYNC_TEST_BACKEND_DIR (D10)
    gist/
      mod.rs        GistBackend, SyncBackend impl, scratch clone, stage, commit and push
      parse.rs      list parsing, stderr mapping, scope check (was gist_parse.rs)
      parts.rs      data.NNN part writer and chaining reader
      pace.rs       clock and pacer for mutating GitHub calls
      runner.rs     Runner trait and ProcessRunner, spawns gh and git
      fake.rs       cfg(test): fake runner, helpers, gh side tests (check, list, create, describe, delete)
      push_tests.rs cfg(test): push, parts and compare-and-swap tests
      pull_tests.rs cfg(test): pull, missing part and clone lifetime tests
  engine/           uses crypto, the backend trait and its own bookkeeping, never prints
    mod.rs          layer declarations
    plan.rs         pure decision table
    scan.rs         plan inputs from storage, index and remote list
    run.rs          Engine, Options, Report, Item, Outcome, SyncError
    push.rs         push of one session, compare-and-swap, pending resume
    pull.rs         pull of one session, verify, backup rotation, atomic write
    fail.rs         Fail and Stop, why one push or pull did not finish
    record.rs       index entry written after a push, refresh or pull
    index.rs        .agr-sync.json load and save, INDEX_FILE, LOCK_FILE
    lock.rs         .agr-sync.lock, pid inside, stale pid reclaimed
  app/              command-facing glue, the only place that picks a concrete backend
    mod.rs          layer declarations
    setup.rs        backend choice, key resolution, error explanations, key import and export
    key.rs          sync.key file, 0600, load, save_new, import
    auto/
      mod.rs        hook side: decide, after_recording, on_path, spawn_detached
      child.rs      child side: run, error_line, take_error_line
    format/
      mod.rs        label, status lines, shared helpers, test fixtures
      summary.rs    preview, privacy, problem, delete preview, rate limit, summary lines
```

### Dependency rules

1. **crypto** uses no `crate::sync` path, and no file outside `crypto/` names
   `chacha20poly1305`, `aead` or `sha2`. As such the open #167 decision (0.10
   `stream` or 0.11 plus our own STREAM layer) touches `crypto/` and `Cargo.toml` only, and the
   known-answer and nonce layout tests in `crypto/blob/tests.rs` prove the wire format stayed.
   `data_encoding` (base32 in `key.rs`) is plain encoding, not part of #167, and stays allowed.
2. **backend** uses nothing from crypto, engine or app (D-rule from Variant A, backends only ever
   see opaque tokens and sealed bytes). `gist/` uses `backend/mod.rs` and its own files.
3. **engine** uses crypto, `crate::sync::backend` (the trait module root only, never `gist`,
   `memory` or `file`), plus `crate::files` and `crate::storage`. Never app.
4. **app** may use every layer and `crate::config`. `setup.rs` is the only file that builds a
   concrete backend.
5. **Outside sync** (`src/commands`, `src/recording`, `tests/`) uses `agr::sync::<item>` or
   `agr::sync::testing::<item>` only, never a layer path.
6. Cross-layer imports are written `crate::sync::<layer>::...`, inside a layer `super::...`,
   never `super::super` (keeps the check below a plain text match).
7. `Command::new` only in `backend/gist/runner.rs` (gh, git) and `app/auto/mod.rs` (the
   detached `agr sync push --auto` child), as the detached child is the auto push feature and not
   gist transport.

Rust has no visibility between sibling modules of one crate that could enforce rules 1 to 4, as
such `sync/mod.rs` carries one `#[cfg(test)]` test that reads `src/sync/<layer>/**/*.rs` and
fails on a forbidden path (rules 1 to 3, 6 and 7). About 40 lines, runs
in `cargo test --lib`, counted by Codecov.

```mermaid
flowchart TD
    ext[commands, recording, tests] --> facade[sync facade]
    facade --> app
    facade --> engine
    facade --> trait
    facade --> crypto
    app --> engine
    app --> gist
    app --> crypto
    engine --> crypto
    engine --> trait[backend trait]
    gist[gist, memory, file] --> trait
```
Legend: an edge means "may import" (facade edges are re-exports only), anything without an edge is forbidden and fails the layer check test.

### Public facade

`agr::sync` (always compiled):
- **engine**: `Engine`, `Options`, `Report`, `Item`, `Outcome`, `SyncError`, `Decision`,
  `SkipReason`, `Duplicate` (the last three are pub fields of `Item` and `Report`).
- **backend**: `SyncBackend`, `BackendError`, `RemoteEntry`, `BlobSource`.
- **crypto**: `Key`, `PhraseError` (inside `setup::ImportError`).
- **app namespaces**, kept as modules because commands call them as `format::status_lines` and so on:
  `auto` (after_recording, spawn_detached, run, error_line, take_error_line),
  `format` (label, status_lines, preview_lines, privacy_lines, problem_lines,
  delete_preview_lines, rate_limit_lines, summary_lines),
  `setup` (default_key_path, resolve_key, explain, open_backend, needs_privacy_ack, import_key,
  export_key, KeyNeed, KeyState, SetupError, ImportError, IMPORT_HINT),
  `key` (save_new, Imported, KeyError).

`agr::sync::testing` (`#[doc(hidden)]`, always compiled, for `tests/` only):
- **backends**: `MemoryBackend`, `GistBackend` (live test), `push_bytes`, `pull_bytes`.
- **crypto**: `seal`, `seal_meta`, `open_meta`, `sha256_hex`, `Meta`, `CryptoError`,
  `seal_stream`, `open_stream`, `BlobError`.
- **engine internals**: `index` (module: load, path_for, Index, IndexEntry, LOCK_FILE),
  `decide`, `resolve`, `Command`, `Remote`, `scan`, `Row`.

Layer modules are private (`mod crypto`) in `sync/mod.rs`. Items are `pub` only when the facade
or `testing` re-exports them, `pub(in crate::sync)` when another layer uses them, `pub(super)`
inside a layer, private otherwise. A `pub` item that names a `pub(in crate::sync)` type fails
`private_interfaces` under `-D warnings`, and with the layers private `dead_code` now reports
items nobody uses (delete them, no deferral).

### Options weighed

1. **Five layers as drafted** (crypto with the key file, backend, `state/` with index, record
   and lock, engine, app). Rejected for three reasons:
   1. `record.rs` uses `engine::fail` and `lock.rs` returns `engine::run::SyncError`, so a state
      layer below the engine needs two code changes only to justify the folder.
   2. The key file does disk IO and permission checks, in crypto it would make the #167 folder
      depend on `crate::files` and the filesystem. Only setup and the commands read the key file,
      the engine gets a `&Key`, as such it belongs to app.
   3. `state` collides with `crate::config::state::State`, which app already imports.
2. **Four layers, engine owns its bookkeeping** (chosen). index, record and lock are only used by
   the engine and its tests, they move with it and every move is a plain rename. The
   crypto folder isolates #167, nothing outside it names a crypto crate.
3. **Stay flat, tighten visibility and split the five big files**. Least churn, but the user
   asked for structure, and with 34 files after the splits flat is worse than today.
4. **Separate `agr-sync` crate in a workspace**. The compiler would enforce the layers, but the
   engine needs `StorageManager`, `files::codec` and config from the main crate, Codecov runs on
   `--lib --test integration`, and CI, MSRV and release setup would all change. Too much for a
   folder problem.
5. **A `cipher.rs` inside crypto that owns every AEAD call** (one file for #167 instead of one
   folder). Weighed and dropped: it needs a new frame sealer type between `blob` and the cipher,
   which is a behavior refactor of the most sensitive code in the PR, only to shrink #167 from
   four files to one. The folder boundary already keeps #167 away from the rest of sync.

For the test access, a cargo feature `testing` with a self dev-dependency was weighed against
the always-on hidden module. The feature hides test helpers from release builds, but
`MemoryBackend` and `FileBackend` ship in release today anyway (D10), tarpaulin and the e2e build
would need the feature too, and a self dev-dependency confuses `cargo +1.88 check --locked`.
Chosen: `#[doc(hidden)] pub mod testing`.

### How the big files split

- **`gist.rs` 1028**: production code (about 336 lines) stays whole in `gist/mod.rs`, the 690
  lines of tests split into `fake.rs` (fake runner, helpers, gh side tests), `push_tests.rs` and
  `pull_tests.rs`. All three are `#[cfg(test)]`.
- **`blob.rs` 551**: production code (about 190 lines) stays in `blob/mod.rs`, the 360 lines of
  tests move unchanged to `blob/tests.rs` (`#[cfg(test)] mod tests;`).
- **`crypto.rs` 434**: `mod.rs` (Key, CryptoError, hashes and their tests), `seal.rs` (v1 header,
  one-shot seal and open, meta token and their tests), `phrase.rs` (phrase and its tests).
- **`format.rs` 511**: status lines in `format/mod.rs`, everything printed around a push, pull
  or delete in `format/summary.rs`.
- **`auto.rs` 429**: hook side (runs in the recording process) in `auto/mod.rs`, child side
  (runs in `agr sync push --auto`) in `auto/child.rs`.

Every file ends up at about 400 lines or less, `run.rs` (369) and `memory.rs` (371) stay as
they are.

### Migration

1. **Moves** (one stage per layer, crypto, backend, engine, app): `git mv`, moved files switch
   their imports, `sync/mod.rs` gets temporary `pub use` shims for the old paths, as such
   commands, recording, tests and unmoved files compile untouched.
2. **Splits** (crypto, gist, app, parallel, disjoint files): layer `mod.rs` re-exports keep
   every path stable.
3. **Facade**: layers private, shims removed, consumers switch to the facade and `testing`.
4. **Visibility and layer check**: item visibility tightened, the layer check test added.
5. **Docs and final verification**.

Every stage compares, against its own parent commit, the sorted test leaf names
(`cargo test --all-targets -- --list`, module path stripped, duplicates kept) and the `shasum`
of every `.snap` file. Both must be identical, the only allowed difference in the whole
refactor is the one new layer check test in Stage 40. Module prefixes of test names do change
with the moves, that is the point of the moves.

### Consequences and open points

- **PR size**: #166 is at 90 changed files vs main, 24 of them in `src/sync/`. None of the sync
  files is on main, as such a move does not add a file, but the target tree has 34 files (the
  layer check and `testing` live in `sync/mod.rs`), so #166 lands at exactly 100, the CodeRabbit
  limit with zero headroom. Options: (a) review Stages 32 to 41 as a stacked PR
  `refactor/sync-modules` against `feature/sync-gist` (about 45 files), so CodeRabbit sees the
  refactor on its own whatever #166 ends at, (b) stay on #166 and drop `gist/fake.rs` by keeping
  the fake runner in `push_tests.rs` (one file of headroom). Architect recommendation: (a).
  **Open for the user.**
- **#167**: whichever way it goes, the change stays in `crypto/` plus `Cargo.toml` and `Cargo.lock`.
- **ADR Module layout table** above is superseded by this refinement, marked in Stage 41.
- **Stage 24 grep rule** (`Command::new` only in `runner.rs`) becomes rule 7 above.
