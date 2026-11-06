# Plan: Encrypted Session Sync (secret gists first)

References: ADR.md, REQUIREMENTS.md

Every stage is TDD (RED test first, GREEN minimal code, REFACTOR), ends with `cargo fmt`,
`cargo clippy -- -D warnings` and `cargo test`. No trivial closures where a method reference
works (SonarCloud rust:S1612). New files stay under ~400 lines, functions ~20 lines, nesting
max 3. Library code never prints (commands do). Library unit tests sit inline in the new
`src/sync/*` files (counted by Codecov through `--lib`), command and end-to-end tests go in
`tests/integration/`. Nothing in the default test run talks to the network or spawns `gh`.

## Open Questions

1. Binary git push to a gist round-trips byte-identical: verify in Stage 7 (live, ignored).
2. Gist description length fits `agr:v1 <token>` (~260 chars): verify in Stage 7.
3. Error text of a git push without credential helper (https protocol): verify in Stage 7.
4. Resolved 2026-11-06 by the user: D1 dedupe report only, D2 no `parts` in the manifest,
   D3 pace only (no stop after 450 calls), D4 no retry-after parsing, "run again later",
   D5 v1 gists left alone.
5. Real non-fast-forward and 404 stderr of `git push` and `gh` against a gist: Stage 18 live.

## Stages

### Stage 1: Crypto module and dependencies

Goal: `src/sync/crypto.rs` with seal/open, metadata token, recovery phrase and SHA-256. No
callers yet.
Owner: implementer

- [x] Add `chacha20poly1305 = "0.10"`, `sha2 = "0.10"`, `data-encoding = "2"`.
- [x] RED/GREEN: `sha256_hex` of known input matches the NIST test vector for "abc".
- [x] RED/GREEN: `seal(key, aad, plain)` then `open` returns the input, output starts with
      `AGR1` + version 1, two seals of the same input differ (random nonce).
- [x] RED/GREEN: open fails on a flipped ciphertext byte, flipped tag, wrong key, wrong AAD,
      truncated blob, unknown version, each with a distinct error.
- [x] RED/GREEN: the sealed bytes of a gzip session contain neither `1f 8b` at offset 0 nor
      any substring of the session text.
- [x] RED/GREEN: `Meta { v, path, sha256, size }` round-trips through `seal_meta`/`open_meta`
      as base64url, token for another key gives `OtherKey`.
- [x] RED/GREEN: phrase export/import round-trips, accepts lowercase, spaces and missing dashes,
      rejects wrong prefix, wrong length and a single changed character (checksum).
- [x] `Key::generate()` from `OsRng`, `Debug` prints `Key(..)` never the bytes.
- [x] `cargo check --target x86_64-pc-windows-msvc` passes.

Files: `Cargo.toml`, `Cargo.lock`, `src/lib.rs`, `src/sync/mod.rs`, `src/sync/crypto.rs`
Depends on: none

Considerations:
- AAD: `b"agr:blob:" + path` for blobs, `b"agr:meta"` for tokens (ADR "Blob and token format").

### Stage 2: Config v5 and state fields

**Update 2026-11-06**: auto-optimize (#184) took config v5 on main, as such the sync migration is now `migrations/v6.rs` with `CURRENT_VERSION = 6`. References to v5 below mean v6.

Goal: `[sync] backend` / `auto_push`, migration v5, `State` gets `sync_privacy_ack` and
`last_auto_push_error`.
Owner: implementer

- [x] RED: `migrate_config("config_version = 4\n")` yields `[sync]` with `auto_push = true`, a
      commented `# backend = "gist"` line, `config_version = 5`, sync stays off.
- [x] RED: running v5 twice changes nothing, an existing `backend = "gist"` and
      `auto_push = false` are kept.
- [x] RED: `backend = "s3"` fails with a message naming `gist`.
- [x] GREEN: `SyncConfig`, `SyncBackendKind { Gist }` (`rename_all = "lowercase"`),
      `migrations/v5.rs`, register it, `CURRENT_VERSION = 5`.
- [x] RED/GREEN: `State` round-trips the two new fields, an old `state.toml` without them loads.
- [x] Field docs for `sync.backend` and `sync.auto_push`, accept the default-config snapshot
      (only the new section may change).

Files: `src/config/types.rs`, `src/config/mod.rs`, `src/config/state.rs`,
       `src/config/migrate/mod.rs`, `src/config/migrate/migrations/mod.rs`,
       `src/config/migrate/migrations/v5.rs`, `src/config/docs/fields.rs`,
       `tests/integration/snapshots/agr__config__migrate__tests__complete_default_config_snapshot.snap`,
       `tests/integration/config_test.rs`
Depends on: none

### Stage 3: Raw atomic write

Goal: `codec::atomic_write_raw(path, bytes)`, same temp, fsync, rename and dir sync as
`atomic_write`, never encodes.
Owner: implementer

- [x] RED: writing gzip bytes to `x.cast.gz` leaves the exact bytes (no double gzip), the temp
      is hidden and removed, a failing write leaves no temp and no target.
- [x] GREEN: share `write_target` internals, no copy-paste of the rename logic.

Files: `src/files/codec.rs`, `tests/integration/codec_test.rs`
Depends on: none

### Stage 4: Key file and sync index

Goal: `src/sync/key.rs` (load, create 0600, refuse loose permissions, replace with force) and
`src/sync/index.rs` (load, save, missing or corrupt means empty plus warning value).
Owner: implementer

- [x] RED/GREEN: `save_new` creates the file with mode 0600 (unix), `load` refuses mode 0644
      with a message naming `chmod 600`, missing file gives `None`.
- [x] RED/GREEN: `import` refuses a different existing key without force, accepts the same key,
      replaces with force.
- [x] RED/GREEN: index round-trips entries, unparseable file loads empty and returns a warning
      string, save goes through `atomic_write`, file is `<storage>/.agr-sync.json`.
- [x] RED/GREEN: `list_sessions` and `get_stats` ignore `.agr-sync.json` and `.agr-sync.lock`.

Files: `src/sync/mod.rs`, `src/sync/key.rs`, `src/sync/index.rs`
Depends on: Stage 1, Stage 3

Considerations:
- Permission code behind `cfg(unix)`, Windows only compiles.

### Stage 5: Backend trait, memory backend and decision table

Goal: `SyncBackend`, `RemoteEntry`, `BackendError`, `MemoryBackend`, pure `plan` function.
Owner: implementer

- [x] RED/GREEN: `MemoryBackend` push without id creates a new id, with id replaces, pull of an
      unknown id errors, delete removes, `fail_on(id)` injects an error for robustness tests.
- [x] RED/GREEN: decision table, one test per ADR row: in sync, local only, remote only,
      local changed, remote changed, both changed, no index plus different hash (conflict), no
      index plus equal hash (in sync), other key, unknown version, plain, locked, over cap.
- [x] RED/GREEN: newer local mtime with remote changed is still a conflict (mtime not an input).
- [x] RED/GREEN: `--overwrite` turns a conflict into push (push) or pull (pull) only for the
      selected sessions.

Files: `src/sync/mod.rs`, `src/sync/backend.rs`, `src/sync/memory.rs`, `src/sync/plan.rs`
Depends on: Stage 4

### Stage 6: Sync engine (scan, push, pull, delete, report)

Goal: `src/sync/scan.rs` builds plan inputs from storage, index and backend list, `src/sync/run.rs`
executes it with per-session errors, sync lock and index updates.
Owner: implementer

- [x] RED: push of three new sessions on `MemoryBackend` uploads three blobs, none starts with
      gzip magic, the index holds path, hash and id, a second push plans nothing.
- [x] RED: a locally changed session updates the same remote id, no new entry.
- [x] RED: pull on an empty storage dir writes byte-identical `.cast.gz` files, creates agent
      dirs, builds the index, files open with `codec::open_reader`.
- [x] RED: tampered blob, wrong key and hash mismatch are reported as failed, leave no file and
      no temp.
- [x] RED: pull `--overwrite` keeps the old file as hidden `.bak`, without it the file is
      untouched.
- [x] RED: one failing session (`fail_on`) does not stop the others, report counts pushed,
      pulled, skipped, conflicts, failed, bytes.
- [x] RED: dry run returns the same plan and changes nothing locally or in the backend.
- [x] RED: delete removes the remote entry and the index entry and keeps the local file.
- [x] RED: deleted index rebuilds to the same decisions as a second machine.
- [x] RED: a second engine run while `.agr-sync.lock` is held by a live pid returns `Busy`.
- [x] RED: session filter uses the same name lookup as other commands (`resolve_cast_path`).
- [x] GREEN: engine takes `&dyn SyncBackend`, `&Key`, `&StorageManager`, never prints.
- [x] Split `run.rs` into `run.rs` and `pull.rs` if it passes ~400 lines.

Files: `src/sync/mod.rs`, `src/sync/scan.rs`, `src/sync/run.rs`, `src/sync/pull.rs`,
       `src/sync/lock.rs`,
       `tests/integration/sync_engine_test.rs`, `tests/integration.rs`
Depends on: Stage 2, Stage 5

Considerations:
- Index saved after every successful session, not once at the end.
- Size cap 100MB on-disk, checked before reading the file into memory.

### Stage 7: Gist backend

Goal: `GistBackend`, the only code that spawns `gh` and `git`.
Owner: implementer

- [x] RED/GREEN (pure, no process): parse `gh api gists --paginate` JSON into `RemoteEntry`
      filtered on the `agr:v1 ` prefix, map gh/git stderr samples to `BackendError` variants
      (`not logged in`, missing `gist` scope, network, credential helper hint `gh auth setup-git`).
- [x] `check`: `gh` and `git` on PATH, `gh auth status` exit code, scope line contains `gist`.
      Never pass or capture the token.
- [x] `push` new: `gh gist create` with `agr-session.json` (`{"format":1}`) and the description,
      then shallow clone to a temp dir under `std::env::temp_dir()`, write `session.agr`,
      commit, push, remove the temp dir (also on error).
- [x] `push` existing: clone, replace, commit, push. `pull`: clone, read `session.agr`.
      `delete`: `gh gist delete <id> --yes`.
- [x] Live test `tests/sync_gist_live.rs`, `#[ignore]`: create, push a 2MB random binary blob,
      list, pull, compare bytes, update, delete. Run it once by hand (NOT run in this session, uploads
      are blocked by policy) and record the result for Open Questions 1 to 3 in this PLAN. If binary push fails, switch the blob to base64url text
      inside `gist.rs` only.

Files: `src/sync/mod.rs`, `src/sync/gist.rs`, `src/sync/gist_parse.rs`, `src/sync/runner.rs`,
       `src/sync/setup.rs`,
       `tests/sync_gist_live.rs`
Depends on: Stage 6

### Stage 8: Sync and key commands

Goal: `agr sync status|push|pull|delete` and `agr sync key export|import`.
Owner: implementer

- [x] RED: clap parse tests for every subcommand, `--dry-run`, `--yes`, `--overwrite`,
      `[session...]`, `key import --force`, hidden `push --auto`.
- [x] RED: sync unset prints how to enable `[sync] backend = "gist"`, exits non-zero, no `gh`
      spawned (PATH without `gh` in the test proves it).
- [x] RED: `gh` missing and not logged in give the two hint lines, exit non-zero, nothing
      changes (fake `gh` script on PATH in a temp dir).
- [x] RED: no TTY without `--yes` aborts push and pull, `status` and `--dry-run` work.
- [x] RED: first creating push shows the privacy and key backup warning, needs confirmation,
      stores `sync_privacy_ack`, not asked again. No key and remote agr gists present aborts
      with the import hint.
- [x] RED: `status` output per state with totals, backend name, remote count, `NO_COLOR` gives
      no escape codes, prints and clears `last_auto_push_error`.
- [x] RED: `key export` prints the phrase, `key import` reads it from stdin or argument.
- [x] GREEN: `SyncCommands` and `SyncKeyCommands` in `src/cli/subcommands.rs`, one `Sync`
      variant in `src/cli.rs`, handlers in `src/commands/sync/`, output via theme helpers,
      prompts via `should_proceed`.
- [x] Accept new help and completions snapshots (only sync additions may change).

Files: `src/cli.rs`, `src/cli/subcommands.rs`, `src/dispatch.rs`, `src/commands/mod.rs`,
       `src/sync/file.rs`, `src/sync/format.rs`,
       `src/commands/sync/mod.rs`, `src/commands/sync/key.rs`,
       `tests/integration/cli_sync_test.rs`, `tests/integration/snapshot_cli_test.rs`,
       `tests/integration.rs`, `tests/integration/snapshots/integration__snapshot_cli_test__cli_help_main.snap`,
       `tests/integration/snapshots/integration__snapshot_cli_test__cli_help_sync.snap`,
       `tests/integration/snapshots/completions/`
Depends on: Stage 7

Considerations:
- `src/cli.rs` is 420 lines, only the variant and its doc go there, everything else in
  `subcommands.rs`.

### Stage 9: Auto push after recording

Goal: push the finished session in a detached child, never block or change the exit status.
Owner: implementer

- [x] RED/GREEN: `auto::decide(config, state, key_present, path)` returns `Skip(hint)` for no
      backend (silent), `auto_push = false` (silent), no privacy ack (hint to run
      `agr sync push` once), no key, plain `.cast`, live lock, `gh`/`git` not on PATH, else
      `Spawn(args)` with `sync push --auto <agent>/<name>`.
- [x] RED/GREEN: the `--auto` run never prompts, never pulls or deletes, skips conflicts, writes
      one-line `last_auto_push_error` to state on failure, returns quietly on `Busy`.
- [x] GREEN: hook in `Recorder::finish` after `finalize_recording`, prints at most one hint
      line, spawn with null stdio and `process_group(0)` on unix, no wait.
- [x] `src/recording.rs` stays under ~400 lines (logic lives in `src/sync/auto.rs`).

Files: `src/sync/mod.rs`, `src/sync/auto.rs`, `src/recording.rs`, `src/recording/auto_push.rs`,
       `tests/integration/recording_test.rs`
Depends on: Stage 8

### Stage 10: Docs

Goal: README, config docs and generated docs describe sync, encryption and auto push.
Owner: implementer

- [x] README section "Syncing Sessions": `[sync]`, the commands, key export and import, one
      gist per session, 100MB cap, `gh` and `git` needed, auto push default.
- [x] Privacy statement in README and config docs: secret gists are readable with the link,
      uploads are encrypted locally, gist descriptions hold only an opaque token, losing every
      copy of the key makes synced sessions unrecoverable.
- [x] `cargo xtask gen-docs`, commit the regenerated files.

Files: `README.md`, `src/config/docs/fields.rs`, `docs/COMMANDS.md`, `docs/man/`, `docs/wiki/`
Depends on: Stage 8

### Stage 11: Final verification

Goal: whole feature green, PR within limits.
Owner: implementer

- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.
- [ ] `cargo check --target x86_64-pc-windows-msvc`.
- [ ] Grep the diff: no `println!` or `eprintln!` in `src/sync/`, no token handling.
- [ ] `git diff --stat main` under 100 files.
- [ ] Manual run on the maintainer machine: push two sessions, pull into a fresh storage dir,
      compare hashes, `agr sync delete` both.

Files: `.state/feature-sync-gist/PLAN.md`
Depends on: Stage 9, Stage 10

### Stage 12: Fix round before the PR

Goal: architect and product owner findings, strict TDD.
Owner: implementer

- [x] RED/GREEN: create with `agr:pending`, token set after the blob push, orphan not listed.
- [x] Tests: token never leaks, `push --overwrite`, cleanup and vanished gist, unknown version
      and other key left alone, no TTY pull aborts, pulled `.cast.gz` plays and lists.
- [x] Test backend announcement, `.bak` rotation, delete re-upload line.
- [x] `spawn_detached` into `runner.rs`, `confirm_planned`, function size refactors, 0700
      scratch dir, `key import` stdin docs, lock race `ponytail:` note.
- [x] Docs: field docs, `sync key` help, README, ADR (Accepted, decisions 9 and 10).

Files: `src/sync/*`, `src/commands/sync/run.rs`, `src/cli/subcommands.rs`,
       `src/recording/auto_push.rs`, `src/config/docs/fields.rs`, `README.md`, `docs/`,
       `tests/integration/cli_sync_test.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 10

### Stage 13: Large sessions and listing limit

Goal: parts in the same gist, 1 GiB cap, listing abort at 3000, neutral file names.
Owner: implementer

- [x] `sync::parts` pure split, join, sequence check, stale parts.
- [x] Gist backend writes `data.NNN` in one commit, removes stale parts, pull joins and
      rejects gaps before returning anything. Stub is `.keep`.
- [x] `MAX_BYTES` 1 GiB, skip text, listing of 3000 or more gists is an error.
- [x] Docs: ADR, README, field docs. Live test for parts, stub and shrink (not run).

Files: `src/sync/parts.rs`, `src/sync/gist.rs`, `src/sync/gist_parse.rs`, `src/sync/scan.rs`,
       `src/sync/run.rs`, `src/sync/auto.rs`, `src/config/docs/fields.rs`, `README.md`, `docs/`,
       `tests/sync_gist_live.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 12

### Stage 14: Manifest v2 and gen in the blob AAD

Goal: `Meta { v: 2, path, sha256, size, gen }`, blob bound to header, path and gen.
Owner: implementer

- [x] RED: `blob_aad(path, gen)` is `AGR1 || 0x01 || agr:blob: || path || : || gen`, a blob
      sealed for gen 3 does not open with gen 2 or with another path.
- [x] RED: a v1 token is `UnknownVersion` in `scan`, never touched (D5).
- [x] GREEN: `Meta` gets `gen`, `v` is 2, push seals gen 1 for now (engine CAS comes in
      Stage 20), pull opens with the manifest gen. KAT vector for `seal` unchanged.
- [x] Update the engine test helpers that build tokens and blobs by hand.

Files: `src/sync/crypto.rs`, `src/sync/scan.rs`, `src/sync/run.rs`, `src/sync/pull.rs`,
       `tests/integration/sync_engine_test.rs`
Depends on: Stage 9, Stage 13

### Stage 15: UTC commit dates through the runner

Goal: no timezone in gist commits, still no env mutation in tests.
Owner: implementer

- [x] RED (fake runner): `git commit` gets `GIT_AUTHOR_DATE` and `GIT_COMMITTER_DATE` as
      `@<secs> +0000`, every other call gets no extra env.
- [x] GREEN: `Runner::run` takes `env: &[(&str, String)]`, `ProcessRunner` passes it with
      `Command::env`, the date comes from a clock argument so the test is fixed.
- [x] Commit identity stays `agr` / `agr@localhost`.

Files: `src/sync/runner.rs`, `src/sync/gist.rs`
Depends on: Stage 9, Stage 13

### Stage 16: Rate limit error and pacer

Goal: a distinct error that stops the run, and a pure pacer for mutating calls.
Owner: implementer

- [x] RED: `map_failure` maps `HTTP 403` or `HTTP 429` with `rate limit`, and
      `secondary rate limit`, to `BackendError::RateLimited`, a plain 403 stays as it is.
- [x] RED: `explain(RateLimited)` gives one line plus "run again later".
- [x] RED: `Pacer` with an injected clock and sleeper waits so two mutating calls are at least
      one second apart (D3: pace only, no call budget per run).
- [x] GREEN: `src/sync/pace.rs` (pure, no thread sleep in tests), new variant, mapping.

Files: `src/sync/backend.rs`, `src/sync/gist_parse.rs`, `src/sync/pace.rs`, `src/sync/mod.rs`,
       `src/sync/setup.rs`
Depends on: Stage 9, Stage 13

### Stage 16b: Batch 10 review fixes

Goal: fix the pair review findings of Stages 14 to 16 before the trait reshape.
Owner: implementer

- [x] RED/GREEN: `git push` `returned error: 429` maps to `RateLimited`, match narrowed to
      `api rate limit`, `secondary rate limit`, `http 429`, `error: 429`.
- [x] RED/GREEN: git push rejection maps to new `BackendError::Rejected`, Stage 17 maps it to
      `Changed(None)` (Stage 18 test below).
- [x] RED/GREEN: `git commit` env pins author and committer name and email, `commit.gpgsign=false`.
- [x] GREEN: one `Clock` trait for pacing and commit dates, `Mutex<Pacer>` in `GistBackend`,
      pre-epoch clock is an error.
- [x] RED/GREEN: `gen == 0` is `UnknownVersion`, raw legacy v1 JSON without `gen` is tested.
- [x] ADR wording for the blob AAD.

Files: `src/sync/gist_parse.rs`, `src/sync/gist.rs`, `src/sync/pace.rs`, `src/sync/backend.rs`,
       `src/sync/scan.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 14, Stage 15, Stage 16

### Stage 17: Backend trait reshape and memory compare-and-swap

Goal: `create`, `push(id, expected, manifest, blob)`, `describe`, `pull` with manifest,
`created_at`, `Changed` and `NotFound`, memory and file backends implement the swap.
Owner: implementer

- [x] RED (memory): push with the current token as `expected` stores blob and manifest, the
      description stays until `describe`. Push with a stale `expected` returns
      `Changed(Some(current))` and stores nothing. `create` gives a pending, unlisted id.
      Unknown id is `NotFound`. `created_at` grows with each create.
- [x] RED (memory): test hooks to change the stored manifest, blob or description by hand
      (tamper, rollback, stalled description).
- [x] GREEN: trait as in the ADR, memory under one mutex (models git's ref check), file
      backend gets the same rules (`<id>.manifest`, `<id>.desc`).
- [x] Map `BackendError::Rejected` to `Changed(None)` in the trait reshape.
- [x] Engine call sites adapted without new behaviour: create then push with `expected` from
      the listing, then describe. Gist backend adapted mechanically (`create_gist` and
      `set_description` become `create` and `describe`, no delete on error), its compare-and-swap
      follows in Stage 18 right after this one.

Files: `src/sync/backend.rs`, `src/sync/memory.rs`, `src/sync/file.rs`, `src/sync/run.rs`,
       `src/sync/pull.rs`, `src/sync/gist.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 14, Stage 15, Stage 16b

### Stage 18: Gist compare-and-swap, manifest, created_at and pacing

Goal: the real backend does what the memory backend models.
Owner: implementer

- [x] RED (fake runner): push clones, reads `manifest`, a mismatch returns `Changed(Some(..))`
      with no `git add`, `commit` or `push` call after the clone.
- [x] RED: the push args never contain `--force`, `-f` or a `+` refspec, and a rejected push
      (`! [rejected]`, `non-fast-forward`, `fetch first`) maps to `Changed(None)` with no retry.
- [x] RED: `manifest` is staged in the same `git add` and commit as the parts.
- [x] RED: pull returns the manifest from the same clone as the parts, missing manifest is `None`.
- [x] RED: `parse_list` reads `created_at`, a 404 maps to `NotFound`.
- [x] RED: every mutating call (create, `git push`, PATCH, delete) goes through the pacer.
- [x] GREEN, then extend the ignored live test: create, push, stale push gets `Changed`,
      description lags and is repaired.

Files: `src/sync/gist.rs`, `src/sync/gist_parse.rs`, `tests/sync_gist_live.rs`
Depends on: Stage 17

### Stage 19: Index fields, index id fallback and duplicate pick

Goal: rows know the last gen, token and pending state, the listing is never the only source.
Owner: implementer

- [x] RED: an old index without `gen`, `token`, `pending` still loads (serde defaults).
- [x] RED: a path in the index with a `remote_id` that the listing misses gets that id and the
      index token as the planned remote, a pending entry gets its id and no token.
- [x] RED: two remote entries for one path pick the oldest `created_at`, id as tie-break, the
      same on every machine. Same SHA-256 means the others are listed in a note, different
      SHA-256 is `Decision::Duplicate`, never acted on, also not with `--overwrite` (D1).
- [x] RED: a description gen lower than the index gen marks the row for description repair.
- [x] GREEN: `index.rs`, `scan.rs`, `plan.rs`, text in `format.rs`.

Files: `src/sync/index.rs`, `src/sync/scan.rs`, `src/sync/plan.rs`, `src/sync/format.rs`
Depends on: Stage 17

### Stage 19b: Batch 12 review fixes

Goal: gen and token are persisted, a deleted gist is never recreated silently (D6 to D8).
Owner: implementer

- [x] RED: after push, refresh and pull the index entry has gen and token, the fallback push
      sends `expected = Some(token)`.
- [x] RED: git "Repository not found" is not `NotFound`, only the `gh` API 404 is.
- [x] RED: push or pull of a deleted gist is "remote copy gone", skipped, nothing created,
      `push --overwrite` recreates it (D6).
- [x] RED: missing `created_at` sorts last in the pick (D7), late pending pick documented (D8).
- [x] RED: an index id listed as other key or unknown version is not pushed into.
- [x] RED: duplicate status label "duplicate", asserted on the real status line.

Files: `src/sync/record.rs`, `src/sync/fail.rs`, `src/sync/run.rs`, `src/sync/pull.rs`,
       `src/sync/scan.rs`, `src/sync/gist_parse.rs`, `src/sync/format.rs`
Depends on: Stage 18, Stage 19

### Stage 20: Engine push as compare-and-swap, pending resume, rate limit stop

Goal: no lost update, no orphan, no stuck gist on push.
Owner: implementer

- [x] RED: push uses gen = planned gen + 1 and the planned token as `expected`, `Changed` is
      reported as "changed remotely, pull first" and nothing is recorded.
- [x] RED: `Changed(Some(t))` where `t` is our local hash is adopted (index and description
      repaired), not a conflict.
- [x] RED: a new session stores the id with `pending` before the push, a failed push leaves
      the index entry and the next run resumes the same id, the backend never gets a second
      `create` for that path.
- [x] RED: `NotFound` on the index id is "remote copy gone" (done in Stage 19b), only `--overwrite` creates once.
- [x] RED: describe failing after a good push still records the push, the next run repairs the
      description, the description is never set to a lower gen.
- [x] RED: `RateLimited` stops the run with `SyncError::RateLimited { left }`, earlier
      sessions stay recorded, the command prints "N left, run again later", auto push stores
      the same line.
- [x] GREEN: push logic moves to `src/sync/push.rs` (keeps `run.rs` under ~400 lines).

Files: `src/sync/push.rs`, `src/sync/run.rs`, `src/sync/mod.rs`, `src/sync/auto.rs`,
       `src/commands/sync/run.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 18, Stage 19, Stage 19b

### Stage 21: Engine pull with manifest and gen check

Goal: rollback and stale copies are rejected, the manifest wins over the description.
Owner: implementer

- [x] RED: a manifest gen lower than the index gen is rejected ("older than the last sync"),
      the local file and the `.bak` are untouched.
- [x] RED: a manifest for another path, or a blob that does not open with the manifest gen,
      is rejected.
- [x] RED: a manifest newer than the description is pulled and the description repaired.
- [x] RED: pull records gen and token in the index.
- [x] GREEN in `pull.rs`.

- [x] Follow-up from Stage 20 (D3/D4): `delete_one` returns `Result<Outcome, Stop>`, a `RateLimited`
      during `agr sync delete` stops the run with `SyncError::RateLimited { left }`, tested.

Files: `src/sync/pull.rs`, `src/sync/run.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 20

### Stage 22: Concurrency test, three machines on one remote

Goal: prove the invariants with interleaved runs, memory backend only.
Owner: implementer

- [x] Three storage dirs and indexes (A, B, C), one shared `MemoryBackend`, one key.
- [x] A test-only wrapper backend runs a closure before the first `push` or `describe` of a
      run, so B's whole push lands between A's listing and A's push, and a dropped `describe`
      models a stalled PATCH.
- [x] Scenarios: A and B edit the same session, both push (one wins, the other is
      `Changed`, after pull and `--overwrite` the result is the one the user chose).
      Interrupted create on A resumed on A (one gist, not two). C with a lost index pulls
      the newest gen. Rollback of the manifest to an older gen is rejected on A and B.
      Stalled description heals on the next run. Rate limit in the middle of a run of three.
- [x] Assertions after every scenario: no lost update (every acknowledged push is either the
      remote content or reported as conflict on the loser), no stuck gist (each path ends with
      one listed gist and nothing pending after one more run), no silent overwrite (a local file
      only changes through a reported pull with a `.bak`).

Files: `tests/integration/sync_multi_machine_test.rs`, `tests/integration.rs`
Depends on: Stage 21

### Stage 23: Docs for Variant A

Goal: users know what sync guarantees and what it does not.
Owner: implementer

- [x] README sync section: changed remotely, duplicates, rate limit stop, known limits list
      from the ADR.
- [x] ADR "To verify" gets the live checks of Stage 18, ADR status stays Proposed until the
      user settles D1 to D5.
- [x] `cargo xtask gen-docs` if help text changed.

Files: `README.md`, `docs/`, `.state/feature-sync-gist/ADR.md`
Depends on: Stage 21

### Stage 25: Variant A review fixes B1 and B2

Goal: the loser of a push race heals the listing, a pending entry is never the last hash, a
rate limit during pull stops the run.
Owner: implementer

- [x] RED: `Changed(Some(theirs))` with another hash, `theirs` opens with our path and gen >=
      max(index gen, row gen): the description is set to `theirs`, the session reports
      CHANGED, nothing recorded. Lower gen, other path or other key: no describe.
- [x] RED: a `RateLimited` from that describe stops the run, any other describe error still
      reports CHANGED.
- [x] RED: a pending index entry (sha256 `""`) gives `last = None` in the scan row.
- [x] RED: `RateLimited` from `backend.pull` stops the run with `SyncError::RateLimited`,
      `NotFound` stays "remote copy gone".
- [x] GREEN in `push.rs` `adopt`, `scan.rs` `row`, `pull.rs` `verified` (`map_err(Fail::from)`).

Files: `src/sync/push.rs`, `src/sync/pull.rs`, `src/sync/scan.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 21

### Stage 26: Streamed blob v2 (STREAM, 64 KiB chunks)

Goal: seal and open a blob from a reader into a writer with bounded memory, ADR D9 format.
Owner: implementer

- [x] `Cargo.toml`: `chacha20poly1305 = { version = "0.10", features = ["stream"] }`, no new crate
      in `Cargo.lock` (check the diff).
- [x] RED: round trip for 0, 1, 65535, 65536, 65537, 3 x 65536 and 3 x 65536 + 5 bytes, sealed
      length is `5 + 19 + n_frames * 16 + len`, an empty input is one 16 byte frame.
- [x] RED: seal returns the plaintext SHA-256 and size, open returns the same pair.
- [x] RED: tamper on a 4 frame blob, each rejected and the writer gets no byte after the
      failing frame: truncate at every frame boundary and inside a frame, drop frame 1, drop the
      last frame, swap frames 1 and 2, duplicate frame 1, append a copy of the last frame,
      append one byte, flip a bit in the prefix, in a frame and in the last tag.
- [x] RED: wrong key, other path, other gen, v1 blob (`UnsupportedVersion(1)`), bad magic,
      under 24 bytes (`Truncated`).
- [x] RED: fixed key and prefix vector (KAT) so the layout cannot drift silently.
- [x] GREEN: new `src/sync/blob.rs` with `seal_stream(key, path, gen, impl Read, impl Write)`,
      `open_stream(key, path, gen, impl Read, impl Write)`, both `-> Result<(String, u64), _>`,
      plus test-friendly `seal_blob` / `open_blob` over byte slices. `sha256_reader` in
      `crypto.rs`. Look-ahead via `BufReader::fill_buf` as in the ADR.

Files: `Cargo.toml`, `Cargo.lock`, `src/sync/blob.rs`, `src/sync/mod.rs`, `src/sync/crypto.rs`
Depends on: Stage 21

### Stage 27: Raw atomic write with a fill closure

Goal: pull can stream into the hidden temp and decide before the rename.
Owner: implementer

- [x] RED: `atomic_write_raw_with(path, |w| ...)` writes through the hidden `.tmp`, renames on
      `Ok`, on `Err` the target is untouched and the temp is removed.
- [x] RED: an existing target is replaced only on `Ok`.
- [x] GREEN in `codec.rs`, `atomic_write_raw` becomes a one-line call of it.

Files: `src/files/codec.rs`, `tests/integration/codec_test.rs`
Depends on: Stage 3


Review notes for Stages 28 and 29 (D9 batch 1 pair review, 2026-11-06), must be followed:
- D11: before the B1 describe in `push.rs` `adopt`, re-read the manifest via the backend and describe only if it still equals the adopted token. Test: a spy backend whose describe first lets another machine push and describe G+1; the description must end at G+1. Document the remaining window in README known limits.
- Push: the manifest (with the plaintext hash) is passed before the blob streams, so compute the hash in a pre-pass (`sha256_reader`), seal in a second pass, compare the seal digest with the pre-hash and abort that session with "session changed while pushing, run again" on mismatch, never push.
- Pull: hash and size checks against the manifest run inside the `atomic_write_raw_with` fill, before rename. The `.bak` rotation happens only after the stream verified and before rename (needs a hook or an explicit order in the fill), a failed pull never replaces the `.bak`.
- The chained part reader never returns `Ok(0)` before all parts are consumed; `fill_buf`/reads retry `Interrupted`.
- `seal_blob` / `open_blob` (whole-blob helpers) never on the production path: make them `#[cfg(test)]` or test-support only.
- Tests to add: one-byte-per-read source and reader, source error mid-stream, destination error mid-stream, `adopt` same-gen different content (comment or test).

### Stage 28: Backend trait moves streams

Goal: no backend holds a whole blob in memory, behaviour otherwise unchanged.
Owner: implementer

- [x] Trait as in ADR D9: `push(.., blob: BlobSource)`, `pull -> (Option<String>, Box<dyn Read>)`.
- [x] RED (gist): the source is not called when `expected` does not match, a source error
      commits and pushes nothing, parts roll over at `part_size` (fake runner, small part
      size), `data.000` exists for an empty blob, stale parts removed, shrink and grow.
- [x] RED (gist): pull reader yields the parts in order, the scratch dir is gone after the
      reader is dropped, missing or extra part still errors before reading.
- [x] RED (file, memory): push and pull round trip through the new signatures, CAS unchanged.
- [x] GREEN: `PartWriter` in `gist.rs` (or `parts.rs` if `gist.rs` passes ~400 lines without
      tests), `split` and `join` removed from `parts.rs`. Engine adapted minimally: push passes
      a source that writes today's sealed bytes, pull reads to end and opens as today.
- [x] Wrapper backend in `tests/integration/helpers/sync_net.rs` and `tests/sync_gist_live.rs`
      follow the new signatures.

Done in two commits. Parts writer and reader live in `parts.rs` (`PartWriter`, `PartReader`, `part_names`), `split` and `join` are gone. Seams for Stage 29: `backend::push_bytes` and `backend::pull_bytes` (whole blob in memory, used by `push.rs`, `pull.rs`, fixtures), `Push.blob: Vec<u8>` in `push.rs`. D11 is in `push.rs` `heal_description` (re-reads via `pull`, README known limit added).

Files: `src/sync/backend.rs`, `src/sync/gist.rs`, `src/sync/parts.rs`, `src/sync/file.rs`, `src/sync/memory.rs`, `src/sync/push.rs`, `src/sync/pull.rs`, `tests/integration/helpers/sync_net.rs`, `tests/sync_gist_live.rs`
Depends on: Stage 25

### Stage 29: Engine streams push, pull and scan

Goal: push and pull never hold a session in memory, pull writes nothing unverified.
Owner: implementer

- [x] RED: push of a session whose file changes after scan fails for that session with
      "session changed while pushing, run again", the remote and the index are unchanged.
- [x] RED: pull of a tampered v2 blob (truncated, frame dropped, hash mismatch against the
      manifest) leaves the local file, the `.bak` and the storage dir unchanged and no hidden
      `.tmp` behind.
- [x] RED: pull of a good blob over an existing file writes the `.bak` first, then the file.
- [x] RED: scan hash of a file equals `sha256_hex` of its bytes (streamed hashing).
- [x] GREEN: push source calls `blob::seal_stream` on the opened file and checks hash and
      size, pull uses `blob::open_stream` inside `atomic_write_raw_with`, then hash, then
      `backup_existing`, scan uses `sha256_reader`. Remove `crypto::blob_aad` and the blob use
      of `seal` / `open` (the meta token keeps them).
- [x] Test fixtures move to `blob::seal_blob` / `open_blob`: `sync_engine_test.rs`,
      `sync_net.rs`, `auto.rs` tests.

Done: push hashes in a pre-pass (`sha256_reader`), the `BlobSource` re-opens the file and runs `seal_stream`, a digest other than the pre-hash fails the source and reports "session changed while pushing, run again". Pull streams `open_stream` into `atomic_write_raw_with`, the fill checks hash and size, flushes, rotates the `.bak`, and only then the temp is renamed (order: verify, .bak, rename). Scan uses `sha256_reader`. `crypto::blob_aad` is gone, `seal_blob` / `open_blob` are `#[cfg(test)]` in `blob.rs` and fixtures in `tests/integration/helpers/sync_net.rs`.

Files: `src/sync/push.rs`, `src/sync/pull.rs`, `src/sync/scan.rs`, `src/sync/crypto.rs`, `src/sync/auto.rs`, `tests/integration/sync_engine_test.rs`, `tests/integration/helpers/sync_net.rs`
Depends on: Stage 26, Stage 27, Stage 28

### Stage 30: Peak memory test for sync

Goal: documented, flat peak memory up to large sessions.
Owner: implementer

- [x] `tests/performance/sync_memory_test.rs`, `#[ignore]`, registered in
      `tests/performance.rs`, reuses `parse_max_rss` from `gzip_memory_test.rs` (move it to
      `tests/performance/mod.rs` if needed).
- [x] 64 MiB and 512 MiB stored gzip of random bytes, release binary under `/usr/bin/time`,
      file backend through `AGR_SYNC_TEST_BACKEND_DIR`, push on home A, key import and pull on
      home B, pulled file equals the source.
- [x] Prints the four peak RSS values, asserts 512 MiB minus 64 MiB run <= 16 MiB for push and
      pull and every run < 64 MiB. Numbers go into the PLAN progress notes.

Files: `tests/performance/sync_memory_test.rs`, `tests/performance.rs`, `tests/performance/mod.rs`, `tests/performance/gzip_memory_test.rs`
Depends on: Stage 29

### Stage 31: Docs for D9

Goal: users and reviewers know the format and the memory bound.
Owner: implementer

- [x] README sync section: sessions are encrypted in 64 KiB chunks, memory stays flat, the
      1 GiB cap is the local `.cast.gz` size.
- [x] ADR D9 status to Accepted. "To verify" points at the Stage 30 RSS numbers, they are not copied
      here as Stage 30 runs in parallel.

Files: `README.md`, `.state/feature-sync-gist/ADR.md`
Depends on: Stage 23, Stage 29

### Stage 24: Final verification for Variant A

Goal: green, within limits, rules hold.
Owner: implementer

- [x] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- [x] `RUSTFLAGS="-D warnings" cargo check --target x86_64-pc-windows-msvc --all-targets`, `cargo +1.88 check --all-targets --locked`, `cargo build --release && ./tests/e2e_test.sh` (109 passed).
- [x] Grep: no `println!` or `eprintln!` in `src/sync/`, no `Command::new` outside
      `src/sync/runner.rs`, no `--force` in git push args (the only other hit is the key replace hint in `key.rs`).
- [x] Changed files vs origin/main under 100 (90).

Files: `.state/feature-sync-gist/PLAN.md`
Depends on: Stage 11, Stage 22, Stage 23, Stage 30, Stage 31

### Stage 32: Module refactor, crypto layer move

Goal: `src/sync/crypto/` exists, pure move, nothing else changes. Starts the module structure
refinement (ADR "Module structure (refinement 2026-11-06)"), only after the user approved it.
Owner: implementer

Rules for Stages 32 to 41: no behavior change. Before each stage, on the parent commit, save the
sorted test leaf names (`cargo test --all-targets -- --list 2>/dev/null | grep ': test$' | sed
's/: test$//; s/.*:://' | sort`, duplicates kept) and `shasum tests/integration/snapshots/*.snap
tests/integration/snapshots/*/*.snap` to `/tmp`, after the stage both must be identical (only
Stage 40 adds one test name). Every stage ends with the full gates from the sdlc rules
(fmt, clippy all-targets, test, Windows check, MSRV 1.88). Moves use `git mv` so history follows.

- [ ] `git mv src/sync/crypto.rs src/sync/crypto/mod.rs`, `git mv src/sync/blob.rs src/sync/crypto/blob.rs`.
- [ ] `crypto/mod.rs` declares `pub mod blob;`, `crypto/blob.rs` imports `super::{CryptoError, Key}`.
- [ ] `sync/mod.rs`: `pub use crypto::blob;` shim, as such every `sync::blob` path still compiles.
- [ ] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/crypto.rs`, `src/sync/crypto/mod.rs`, `src/sync/blob.rs`, `src/sync/crypto/blob.rs`
Depends on: Stage 24

### Stage 33: Module refactor, backend layer move

Goal: `src/sync/backend/` with the trait, the fakes and `gist/`, pure move.
Owner: implementer

- [ ] `git mv`: `backend.rs` to `backend/mod.rs`, `memory.rs` and `file.rs` to `backend/`,
      `gist.rs` to `backend/gist/mod.rs`, `gist_parse.rs` to `backend/gist/parse.rs`,
      `parts.rs`, `pace.rs` and `runner.rs` to `backend/gist/`.
- [ ] Moved files fix their own `super::` paths (no `super::super` across layers, see ADR rule 6).
- [ ] `sync/mod.rs` shims: `pub use backend::{file, gist, memory};` and
      `pub use backend::gist::{pace, parse as gist_parse, parts, runner};`.
- [ ] Grep: no file in `src/sync/backend/` names `crypto`, `engine` or `app`.
- [ ] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/backend.rs`, `src/sync/backend/mod.rs`, `src/sync/memory.rs`, `src/sync/backend/memory.rs`, `src/sync/file.rs`, `src/sync/backend/file.rs`, `src/sync/gist.rs`, `src/sync/backend/gist/mod.rs`, `src/sync/gist_parse.rs`, `src/sync/backend/gist/parse.rs`, `src/sync/parts.rs`, `src/sync/backend/gist/parts.rs`, `src/sync/pace.rs`, `src/sync/backend/gist/pace.rs`, `src/sync/runner.rs`, `src/sync/backend/gist/runner.rs`
Depends on: Stage 32

### Stage 34: Module refactor, engine layer move

Goal: `src/sync/engine/` with plan, scan, run, push, pull, fail, record, index and lock, pure move.
Owner: implementer

- [ ] `git mv` the nine files to `src/sync/engine/`, new `engine/mod.rs` declares them with the
      visibility they have today (`pub` for plan, scan, run, index, lock, private for the rest).
- [ ] Cross-layer imports in the moved files use `crate::sync::crypto` and `crate::sync::backend`.
- [ ] `sync/mod.rs` shims: `pub use engine::{index, lock, plan, run, scan};`.
- [ ] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/engine/mod.rs`, `src/sync/plan.rs`, `src/sync/engine/plan.rs`, `src/sync/scan.rs`, `src/sync/engine/scan.rs`, `src/sync/run.rs`, `src/sync/engine/run.rs`, `src/sync/push.rs`, `src/sync/engine/push.rs`, `src/sync/pull.rs`, `src/sync/engine/pull.rs`, `src/sync/fail.rs`, `src/sync/engine/fail.rs`, `src/sync/record.rs`, `src/sync/engine/record.rs`, `src/sync/index.rs`, `src/sync/engine/index.rs`, `src/sync/lock.rs`, `src/sync/engine/lock.rs`
Depends on: Stage 33

### Stage 35: Module refactor, app layer move

Goal: `src/sync/app/` with setup, key, auto and format, pure move. After this stage `src/sync/`
holds only `mod.rs` and the four layer folders.
Owner: implementer

- [ ] `git mv` `setup.rs`, `key.rs`, `auto.rs`, `format.rs` to `src/sync/app/`, new `app/mod.rs`.
- [ ] `sync/mod.rs` shims: `pub use app::{auto, format, key, setup};`.
- [ ] `ls src/sync` shows `mod.rs crypto backend engine app` only.
- [ ] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/app/mod.rs`, `src/sync/setup.rs`, `src/sync/app/setup.rs`, `src/sync/key.rs`, `src/sync/app/key.rs`, `src/sync/auto.rs`, `src/sync/app/auto.rs`, `src/sync/format.rs`, `src/sync/app/format.rs`
Depends on: Stage 34

### Stage 36: Module refactor, crypto split

Goal: every crypto file about 400 lines or less, paths unchanged through re-exports.
Owner: implementer

- [ ] `crypto/mod.rs` keeps Key, CryptoError, hashes and their tests, `seal.rs` gets the v1
      one-shot seal and open plus the meta token and their tests, `phrase.rs` the phrase and its tests.
- [ ] `crypto/mod.rs` re-exports `seal`, `open`, `seal_meta`, `open_meta`, `Meta`,
      `export_phrase`, `import_phrase`, `PhraseError`, every `crypto::x` path stays valid.
- [ ] `crypto/blob.rs` to `crypto/blob/mod.rs` (`git mv`), its tests move unchanged to
      `crypto/blob/tests.rs` (`#[cfg(test)] mod tests;`).
- [ ] `wc -l` of every file in `src/sync/crypto/` about 400 or less, test names and snap hashes identical.

Files: `src/sync/crypto/mod.rs`, `src/sync/crypto/seal.rs`, `src/sync/crypto/phrase.rs`, `src/sync/crypto/blob.rs`, `src/sync/crypto/blob/mod.rs`, `src/sync/crypto/blob/tests.rs`
Depends on: Stage 35

### Stage 37: Module refactor, gist split

Goal: `backend/gist/mod.rs` holds production code only, its tests split by topic.
Owner: implementer

- [ ] `gist/mod.rs` keeps the 336 production lines and declares `#[cfg(test)] mod fake;`,
      `#[cfg(test)] mod push_tests;`, `#[cfg(test)] mod pull_tests;`.
- [ ] `fake.rs`: fake runner, `ok`, `fail`, `backend`, `scrub`, `data_files`, `remote_parts`,
      `source_of` as `pub(super)`, plus the check, list, create, describe, delete, pacer and
      stderr mapping tests.
- [ ] `push_tests.rs`: push, compare-and-swap, parts, commit date and source tests.
      `pull_tests.rs`: pull, missing part and clone lifetime tests.
- [ ] Every file about 400 lines or less, test names and snap hashes identical.

Files: `src/sync/backend/gist/mod.rs`, `src/sync/backend/gist/fake.rs`, `src/sync/backend/gist/push_tests.rs`, `src/sync/backend/gist/pull_tests.rs`
Depends on: Stage 35

### Stage 38: Module refactor, app split

Goal: auto and format under 400 lines, the detached child spawn sits with auto push.
Owner: implementer

- [ ] `app/auto.rs` to `app/auto/mod.rs` (`git mv`): decide, blocker, after_recording, on_path
      and their tests stay, `app/auto/child.rs` gets run, error_line, take_error_line and their
      tests, re-exported from `auto/mod.rs`.
- [ ] `spawn_detached` moves from `backend/gist/runner.rs` to `app/auto/mod.rs` unchanged,
      `src/recording/auto_push.rs` calls `auto::spawn_detached` (`runner` is not a public name after Stage 39).
- [ ] `app/format.rs` to `app/format/mod.rs` (`git mv`): label, status lines, totals, shared
      helpers and the test fixtures stay, `app/format/summary.rs` gets preview, privacy,
      problem, delete preview, rate limit and summary lines and their tests, re-exported.
- [ ] Every file about 400 lines or less, test names and snap hashes identical.

Files: `src/sync/app/auto.rs`, `src/sync/app/auto/mod.rs`, `src/sync/app/auto/child.rs`, `src/sync/app/format.rs`, `src/sync/app/format/mod.rs`, `src/sync/app/format/summary.rs`, `src/sync/backend/gist/runner.rs`, `src/recording/auto_push.rs`
Depends on: Stage 35

### Stage 39: Module refactor, facade and testing module

Goal: outside code sees only `agr::sync::<item>` and `agr::sync::testing::<item>`, the shims are gone.
Owner: implementer

- [ ] `sync/mod.rs`: layers are private `mod`, the facade re-exports exactly the list in the
      ADR, `#[doc(hidden)] pub mod testing` re-exports the test list, shims from Stages 32 to 35 removed.
- [ ] Every cross-layer import inside `src/sync/` written as `crate::sync::<layer>::...`.
- [ ] `src/commands/sync/{run,key}.rs` and `src/recording/auto_push.rs` use facade paths only,
      the integration tests and `sync_gist_live.rs` use facade or `testing` paths only.
- [ ] Grep: no `sync::(crypto|backend|engine|app|blob|gist|memory|index|plan|scan|run)::`
      outside `src/sync/`.
- [ ] `cargo test --test sync_gist_live --no-run` compiles (ignored live test), test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/crypto/mod.rs`, `src/sync/backend/mod.rs`, `src/sync/engine/mod.rs`, `src/sync/app/mod.rs`, `src/sync/engine/run.rs`, `src/sync/engine/scan.rs`, `src/sync/engine/push.rs`, `src/sync/engine/pull.rs`, `src/sync/app/setup.rs`, `src/sync/app/key.rs`, `src/sync/app/auto/mod.rs`, `src/sync/app/auto/child.rs`, `src/sync/app/format/mod.rs`, `src/sync/app/format/summary.rs`, `src/commands/sync/run.rs`, `src/commands/sync/key.rs`, `src/recording/auto_push.rs`, `tests/integration/sync_engine_test.rs`, `tests/integration/sync_index_fields_test.rs`, `tests/integration/sync_multi_machine_test.rs`, `tests/integration/helpers/sync_net.rs`, `tests/sync_gist_live.rs`
Depends on: Stage 36, Stage 37, Stage 38

### Stage 40: Module refactor, item visibility and layer check

Goal: nothing is wider than it needs to be, the dependency rules are tested.
Owner: implementer

- [ ] Every item in `src/sync/` gets the narrowest visibility that compiles (`pub` only when
      re-exported, `pub(in crate::sync)` across layers, `pub(super)` inside a layer). Items that
      `dead_code` reports now are deleted, not allowed.
- [ ] RED: `layer_rules_hold` in `sync/mod.rs` (`#[cfg(test)]`, reads `src/sync/**/*.rs` from
      `CARGO_MANIFEST_DIR`) fails on a temporary `use crate::sync::engine` in
      `backend/memory.rs`, on `chacha20poly1305` named in `engine/push.rs`, on
      `Command::new` in `engine/run.rs` and on `crate::sync::app` in `engine/scan.rs`. Mutants are not committed.
- [ ] GREEN: the test passes on the real tree. Test names identical plus `layer_rules_hold`.

Files: `src/sync/mod.rs`, `src/sync/crypto/mod.rs`, `src/sync/crypto/seal.rs`, `src/sync/crypto/phrase.rs`, `src/sync/crypto/blob/mod.rs`, `src/sync/backend/mod.rs`, `src/sync/backend/memory.rs`, `src/sync/backend/file.rs`, `src/sync/backend/gist/mod.rs`, `src/sync/backend/gist/parse.rs`, `src/sync/backend/gist/parts.rs`, `src/sync/backend/gist/pace.rs`, `src/sync/backend/gist/runner.rs`, `src/sync/engine/mod.rs`, `src/sync/engine/plan.rs`, `src/sync/engine/scan.rs`, `src/sync/engine/run.rs`, `src/sync/engine/push.rs`, `src/sync/engine/pull.rs`, `src/sync/engine/fail.rs`, `src/sync/engine/record.rs`, `src/sync/engine/index.rs`, `src/sync/engine/lock.rs`, `src/sync/app/mod.rs`, `src/sync/app/setup.rs`, `src/sync/app/key.rs`, `src/sync/app/auto/mod.rs`, `src/sync/app/auto/child.rs`, `src/sync/app/format/mod.rs`, `src/sync/app/format/summary.rs`
Depends on: Stage 39

### Stage 41: Module refactor, docs and final verification

Goal: docs match the tree, all gates green.
Owner: implementer

- [ ] ADR "Module layout" table marked superseded by the refinement, refinement status Accepted.
- [ ] Stage 24 rules re-run with the new paths: no `println!` or `eprintln!` in `src/sync/`,
      `Command::new` only in `backend/gist/runner.rs` and `app/auto/mod.rs`, no `--force` in git push args.
- [ ] Full gates plus `cargo build --release && ./tests/e2e_test.sh`, Codecov patch coverage
      not lower than before Stage 32, `wc -l` of every file in `src/sync/` about 400 or less.
- [ ] Changed files vs origin/main recorded in the progress notes (expected 100, see ADR PR size).

Files: `.state/feature-sync-gist/ADR.md`, `.state/feature-sync-gist/PLAN.md`
Depends on: Stage 40

## Batches

1. Stage 1, Stage 2, Stage 3 (parallel, no shared files)
2. Stage 4
3. Stage 5
4. Stage 6
5. Stage 7
6. Stage 8
7. Stage 9, Stage 10 (parallel)
8. Stage 11
9. Stage 14, Stage 15, Stage 16 (parallel, no shared files)
10. Stage 17
11. Stage 18, Stage 19 (parallel, gist files vs index, scan and plan)
12. Stage 19b
14. Stage 20
14. Stage 21
15. Stage 22, Stage 23 (parallel)
16. Stage 25, Stage 26, Stage 27 (parallel, engine vs new blob module vs codec)
17. Stage 28
18. Stage 29
19. Stage 30, Stage 31 (parallel)
20. Stage 24
21. Stage 32
22. Stage 33
23. Stage 34
24. Stage 35
25. Stage 36, Stage 37, Stage 38 (parallel, crypto vs gist vs app files)
26. Stage 39
27. Stage 40
28. Stage 41

Stages 4 to 7 share `src/sync/mod.rs`, as such they run in order.

## Progress

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | done | also `zeroize = "1"` (already pulled in by the cipher) |
| 2 | done | |
| 3 | done | |
| 4 | done | |
| 5 | done | |
| 6 | done | |
| 7 | done | live test written but not run, see ADR "To verify" |
| 8 | done | test-only `AGR_SYNC_TEST_BACKEND_DIR` file backend (`src/sync/file.rs`) |
| 9 | done | |
| 10 | done | README marks the gist transport as not verified live |
| 11 | done | final gates, see Stage 12 for the fix round |
| 12 | done | fix round before the PR |
| 13 | done | parts, 1 GiB cap, listing limit |
| 14 | done | Variant A |
| 15 | done | |
| 16 | done | |
| 17 | done | one commit, no 17a/17b split. Gist `push` ignores `expected` and `manifest` until Stage 18, test `stage_17_push_does_not_compare_expected_yet` in `gist.rs` flips then. Memory test hook `put` sets a whole remote state without compare |
| 18 | done | |
| 19 | done | |
| 19b | done | Batch 12 review fixes |
| 20 | done | |
| 21 | done | also the delete rate limit stop from Stage 20 |
| 22 | done | three machine concurrency test |
| 23 | done | README, ADR status |
| 24 | done | runs after D9 |
| 25 | done | B1, B2 |
| 26 | done | D9 blob v2 |
| 27 | done | |
| 28 | done | trait streams |
| 29 | done | |
| 30 | done | peak RSS (release, macOS, file backend): push 64 MiB 8077312 B, push 512 MiB 8044544 B, pull 64 MiB 7979008 B, pull 512 MiB 7880704 B, growth about 0, all under 64 MiB |
| 31 | done | README, ADR drift, REQUIREMENTS cap wording. Measured RSS numbers come from Stage 30 |
| 32 | todo | module refactor, waits for ADR approval |
| 33 | todo | module refactor, waits for ADR approval |
| 34 | todo | module refactor, waits for ADR approval |
| 35 | todo | module refactor, waits for ADR approval |
| 36 | todo | module refactor, waits for ADR approval |
| 37 | todo | module refactor, waits for ADR approval |
| 38 | todo | module refactor, waits for ADR approval |
| 39 | todo | module refactor, waits for ADR approval |
| 40 | todo | module refactor, waits for ADR approval |
| 41 | todo | module refactor, waits for ADR approval |
