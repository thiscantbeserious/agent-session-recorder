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

- [x] `git mv src/sync/crypto.rs src/sync/crypto/mod.rs`, `git mv src/sync/blob.rs src/sync/crypto/blob.rs`.
- [x] `crypto/mod.rs` declares `pub mod blob;`, `crypto/blob.rs` imports `super::{CryptoError, Key}`.
- [x] `sync/mod.rs`: `pub use crypto::blob;` shim, as such every `sync::blob` path still compiles.
- [x] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/crypto.rs`, `src/sync/crypto/mod.rs`, `src/sync/blob.rs`, `src/sync/crypto/blob.rs`
Depends on: Stage 24

### Stage 33: Module refactor, backend layer move

Goal: `src/sync/backend/` with the trait, the fakes and `gist/`, pure move.
Owner: implementer

- [x] `git mv`: `backend.rs` to `backend/mod.rs`, `memory.rs` and `file.rs` to `backend/`,
      `gist.rs` to `backend/gist/mod.rs`, `gist_parse.rs` to `backend/gist/parse.rs`,
      `parts.rs`, `pace.rs` and `runner.rs` to `backend/gist/`.
- [x] Moved files fix their own `super::` paths (no `super::super` across layers, see ADR rule 6).
- [x] `sync/mod.rs` shims: `pub use backend::{file, gist, memory};` and
      `pub use backend::gist::{pace, parse as gist_parse, parts, runner};`.
- [x] Grep: no file in `src/sync/backend/` names `crypto`, `engine` or `app`.
- [x] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/backend.rs`, `src/sync/backend/mod.rs`, `src/sync/memory.rs`, `src/sync/backend/memory.rs`, `src/sync/file.rs`, `src/sync/backend/file.rs`, `src/sync/gist.rs`, `src/sync/backend/gist/mod.rs`, `src/sync/gist_parse.rs`, `src/sync/backend/gist/parse.rs`, `src/sync/parts.rs`, `src/sync/backend/gist/parts.rs`, `src/sync/pace.rs`, `src/sync/backend/gist/pace.rs`, `src/sync/runner.rs`, `src/sync/backend/gist/runner.rs`
Depends on: Stage 32

### Stage 34: Module refactor, engine layer move

Goal: `src/sync/engine/` with plan, scan, run, push, pull, fail, record, index and lock, pure move.
Owner: implementer

- [x] `git mv` the nine files to `src/sync/engine/`, new `engine/mod.rs` declares them with the
      visibility they have today (`pub` for plan, scan, run, index, lock, private for the rest).
- [x] Cross-layer imports in the moved files use `crate::sync::crypto` and `crate::sync::backend`.
- [x] `sync/mod.rs` shims: `pub use engine::{index, lock, plan, run, scan};`.
- [x] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/engine/mod.rs`, `src/sync/plan.rs`, `src/sync/engine/plan.rs`, `src/sync/scan.rs`, `src/sync/engine/scan.rs`, `src/sync/run.rs`, `src/sync/engine/run.rs`, `src/sync/push.rs`, `src/sync/engine/push.rs`, `src/sync/pull.rs`, `src/sync/engine/pull.rs`, `src/sync/fail.rs`, `src/sync/engine/fail.rs`, `src/sync/record.rs`, `src/sync/engine/record.rs`, `src/sync/index.rs`, `src/sync/engine/index.rs`, `src/sync/lock.rs`, `src/sync/engine/lock.rs`
Depends on: Stage 33

### Stage 35: Module refactor, app layer move

Goal: `src/sync/app/` with setup, key, auto and format, pure move. After this stage `src/sync/`
holds only `mod.rs` and the four layer folders.
Owner: implementer

- [x] `git mv` `setup.rs`, `key.rs`, `auto.rs`, `format.rs` to `src/sync/app/`, new `app/mod.rs`.
- [x] `sync/mod.rs` shims: `pub use app::{auto, format, key, setup};`.
- [x] `ls src/sync` shows `mod.rs crypto backend engine app` only.
- [x] Test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/app/mod.rs`, `src/sync/setup.rs`, `src/sync/app/setup.rs`, `src/sync/key.rs`, `src/sync/app/key.rs`, `src/sync/auto.rs`, `src/sync/app/auto.rs`, `src/sync/format.rs`, `src/sync/app/format.rs`
Depends on: Stage 34

### Stage 36: Module refactor, crypto split

Goal: every crypto file about 400 lines or less, paths unchanged through re-exports.
Owner: implementer

- [x] `crypto/mod.rs` keeps Key, CryptoError, hashes and their tests, `seal.rs` gets the v1
      one-shot seal and open plus the meta token and their tests, `phrase.rs` the phrase and its tests.
- [x] `crypto/mod.rs` re-exports `seal`, `open`, `seal_meta`, `open_meta`, `Meta`,
      `export_phrase`, `import_phrase`, `PhraseError`, every `crypto::x` path stays valid.
- [x] `crypto/blob.rs` to `crypto/blob/mod.rs` (`git mv`), its tests move unchanged to
      `crypto/blob/tests.rs` (`#[cfg(test)] mod tests;`).
- [x] `wc -l` of every file in `src/sync/crypto/` about 400 or less, test names and snap hashes identical.

Files: `src/sync/crypto/mod.rs`, `src/sync/crypto/seal.rs`, `src/sync/crypto/phrase.rs`, `src/sync/crypto/blob.rs`, `src/sync/crypto/blob/mod.rs`, `src/sync/crypto/blob/tests.rs`
Depends on: Stage 35

### Stage 37: Module refactor, gist split

Goal: `backend/gist/mod.rs` holds production code only, its tests split by topic.
Owner: implementer

- [x] `gist/mod.rs` keeps the 336 production lines and declares `#[cfg(test)] mod fake;`,
      `#[cfg(test)] mod push_tests;`, `#[cfg(test)] mod pull_tests;`.
- [x] `fake.rs`: fake runner, `ok`, `fail`, `backend`, `scrub`, `data_files`, `remote_parts`,
      `source_of` as `pub(super)`, plus the check, list, create, describe, delete, pacer and
      stderr mapping tests.
- [x] `push_tests.rs`: push, compare-and-swap, parts, commit date and source tests.
      `pull_tests.rs`: pull, missing part and clone lifetime tests.
- [x] Every file about 400 lines or less, test names and snap hashes identical.

Files: `src/sync/backend/gist/mod.rs`, `src/sync/backend/gist/fake.rs`, `src/sync/backend/gist/push_tests.rs`, `src/sync/backend/gist/pull_tests.rs`
Depends on: Stage 35

### Stage 38: Module refactor, app split

Goal: auto and format under 400 lines, the detached child spawn sits with auto push.
Owner: implementer

- [x] `app/auto.rs` to `app/auto/mod.rs` (`git mv`): decide, blocker, after_recording, on_path
      and their tests stay, `app/auto/child.rs` gets run, error_line, take_error_line and their
      tests, re-exported from `auto/mod.rs`.
- [x] `spawn_detached` moves from `backend/gist/runner.rs` to `app/auto/mod.rs` unchanged,
      `src/recording/auto_push.rs` calls `auto::spawn_detached` (`runner` is not a public name after Stage 39).
- [x] `app/format.rs` to `app/format/mod.rs` (`git mv`): label, status lines, totals, shared
      helpers and the test fixtures stay, `app/format/summary.rs` gets preview, privacy,
      problem, delete preview, rate limit and summary lines and their tests, re-exported.
- [x] Every file about 400 lines or less, test names and snap hashes identical.

Files: `src/sync/app/auto.rs`, `src/sync/app/auto/mod.rs`, `src/sync/app/auto/child.rs`, `src/sync/app/format.rs`, `src/sync/app/format/mod.rs`, `src/sync/app/format/summary.rs`, `src/sync/backend/gist/runner.rs`, `src/recording/auto_push.rs`
Depends on: Stage 35

### Stage 39: Module refactor, facade and testing module

Goal: outside code sees only `agr::sync::<item>` and `agr::sync::testing::<item>`, the shims are gone.
Owner: implementer

- [x] `sync/mod.rs`: layers are private `mod`, the facade re-exports exactly the list in the
      ADR, `#[doc(hidden)] pub mod testing` re-exports the test list, shims from Stages 32 to 35 removed.
- [x] Every cross-layer import inside `src/sync/` written as `crate::sync::<layer>::...`.
- [x] `src/commands/sync/{run,key}.rs` and `src/recording/auto_push.rs` use facade paths only,
      the integration tests and `sync_gist_live.rs` use facade or `testing` paths only.
- [x] Grep: no `sync::(crypto|backend|engine|app|blob|gist|memory|index|plan|scan|run)::`
      outside `src/sync/`.
- [x] `cargo test --test sync_gist_live --no-run` compiles (ignored live test), test names and snap hashes identical.

Files: `src/sync/mod.rs`, `src/sync/crypto/mod.rs`, `src/sync/backend/mod.rs`, `src/sync/engine/mod.rs`, `src/sync/app/mod.rs`, `src/sync/engine/run.rs`, `src/sync/engine/scan.rs`, `src/sync/engine/push.rs`, `src/sync/engine/pull.rs`, `src/sync/app/setup.rs`, `src/sync/app/key.rs`, `src/sync/app/auto/mod.rs`, `src/sync/app/auto/child.rs`, `src/sync/app/format/mod.rs`, `src/sync/app/format/summary.rs`, `src/commands/sync/run.rs`, `src/commands/sync/key.rs`, `src/recording/auto_push.rs`, `tests/integration/sync_engine_test.rs`, `tests/integration/sync_index_fields_test.rs`, `tests/integration/sync_multi_machine_test.rs`, `tests/integration/helpers/sync_net.rs`, `tests/sync_gist_live.rs`
Depends on: Stage 36, Stage 37, Stage 38

### Stage 40: Module refactor, item visibility and layer check

Goal: nothing is wider than it needs to be, the dependency rules are tested.
Owner: implementer

- [x] Every item in `src/sync/` gets the narrowest visibility that compiles (`pub` only when
      re-exported, `pub(in crate::sync)` across layers, `pub(super)` inside a layer). Items that
      `dead_code` reports now are deleted, not allowed.
- [x] RED: `layer_rules_hold` in `sync/mod.rs` (`#[cfg(test)]`, reads `src/sync/**/*.rs` from
      `CARGO_MANIFEST_DIR`) fails on a temporary `use crate::sync::engine` in
      `backend/memory.rs`, on `chacha20poly1305` named in `engine/push.rs`, on
      `Command::new` in `engine/run.rs` and on `crate::sync::app` in `engine/scan.rs`. Mutants are not committed.
- [x] GREEN: the test passes on the real tree. Test names identical plus `layer_rules_hold`.

Files: `src/sync/mod.rs`, `src/sync/crypto/mod.rs`, `src/sync/crypto/seal.rs`, `src/sync/crypto/phrase.rs`, `src/sync/crypto/blob/mod.rs`, `src/sync/backend/mod.rs`, `src/sync/backend/memory.rs`, `src/sync/backend/file.rs`, `src/sync/backend/gist/mod.rs`, `src/sync/backend/gist/parse.rs`, `src/sync/backend/gist/parts.rs`, `src/sync/backend/gist/pace.rs`, `src/sync/backend/gist/runner.rs`, `src/sync/engine/mod.rs`, `src/sync/engine/plan.rs`, `src/sync/engine/scan.rs`, `src/sync/engine/run.rs`, `src/sync/engine/push.rs`, `src/sync/engine/pull.rs`, `src/sync/engine/fail.rs`, `src/sync/engine/record.rs`, `src/sync/engine/index.rs`, `src/sync/engine/lock.rs`, `src/sync/app/mod.rs`, `src/sync/app/setup.rs`, `src/sync/app/key.rs`, `src/sync/app/auto/mod.rs`, `src/sync/app/auto/child.rs`, `src/sync/app/format/mod.rs`, `src/sync/app/format/summary.rs`
Depends on: Stage 39

### Stage 41: Module refactor, docs and final verification

Goal: docs match the tree, all gates green.
Owner: implementer

- [x] ADR "Module layout" table marked superseded by the refinement, refinement status Accepted.
- [x] Stage 24 rules re-run with the new paths: no `println!` or `eprintln!` in `src/sync/`,
      `Command::new` only in `backend/gist/runner.rs` and `app/auto/mod.rs`, no `--force` in git push args.
- [x] Full gates plus `cargo build --release && ./tests/e2e_test.sh`, Codecov patch coverage
      not lower than before Stage 32, `wc -l` of every file in `src/sync/` about 400 or less.
- [x] Changed files vs origin/main recorded in the progress notes (expected 100, see ADR PR size).

Files: `.state/feature-sync-gist/ADR.md`, `.state/feature-sync-gist/PLAN.md`
Depends on: Stage 40

### Stage 42: Own STREAM module, RED invariant tests first (still on 0.10)

Goal: `crypto/blob/stream.rs` per ADR "Own STREAM layer on chacha20poly1305 0.11", proven
byte-identical to `aead::stream::StreamBE32` while 0.10 is still in the tree. `blob/mod.rs` not
switched yet.
Owner: implementer

- [x] RED: `nonce_layout_is_prefix_counter_be_and_flag` (I1), counters 0, 1, 0x01020304,
      `u32::MAX`, both flags, against hand-built bytes.
- [x] RED: `matches_aead_stream_be32` (I2), `Sealer` output equals `EncryptorBE32` output for
      every size in `SIZES`, `Opener` opens what `EncryptorBE32` sealed.
- [x] RED: `next_is_refused_at_u32_max_and_last_still_works` (I3) for `Sealer` and `Opener`,
      `TooLong`, `buf` untouched, `last` at `u32::MAX` seals under `prefix || ffffffff || 01`.
- [x] RED: `counter_moves_before_the_aead_and_never_repeats` (I10), `failed_open_clears_the_buffer` (I9).
- [x] GREEN: `Stream`, `Sealer`, `Opener` as in the ADR API, `CryptoError::TooLong` in `crypto/mod.rs`.
- [x] Mutants from the ADR invariant table for I1, I3, I9, I10 each fail their test, reverted,
      not committed.
- [x] Existing `blob/tests.rs` and both KAT constants untouched (`git diff` shows nothing there).

Files: `src/sync/crypto/blob/stream.rs`, `src/sync/crypto/mod.rs`
Depends on: Stage 41

### Stage 43: Blob v2 on the own STREAM layer (still on 0.10)

Goal: `blob/mod.rs` uses `Sealer` and `Opener`, `aead::stream` no longer used in production code.
Owner: implementer

- [x] RED: `only_the_final_frame_carries_the_last_flag` (I4) and `empty_last_frame_is_just_a_tag`
      (I8) in `blob/tests.rs`, new tests only, no existing test or vector edited.
- [x] GREEN: `Option<Sealer>` and `Option<Opener>` replace `EncryptorBE32` and `DecryptorBE32`,
      `seal_frame` and `open_frame` stay as dispatchers, `TooLong` passes through, tag failure
      stays `Authentication`.
- [x] Every existing test in `blob/tests.rs` passes unchanged, `KAT_SMALL` and
      `KAT_TWO_FRAMES_SHA256` included.
- [x] Mutants for I4, I5, I6, I7, I8 from the ADR table each fail their named test, reverted.

Files: `src/sync/crypto/blob/mod.rs`, `src/sync/crypto/blob/tests.rs`
Depends on: Stage 42

### Stage 44: chacha20poly1305 0.11, seal.rs and Key on the new API

Goal: the bump, nothing outside `crypto/`, `Cargo.toml` and `Cargo.lock` changes.
Owner: implementer

- [x] `Cargo.toml`: `chacha20poly1305 = { version = "0.11", features = ["zeroize"] }`,
      `cargo update -p chacha20poly1305`, the `Cargo.lock` diff equals the ADR dependency table
      (no other crate moves).
- [x] `stream.rs` on `AeadInOut::encrypt_in_place` and `decrypt_in_place` (no deprecated
      `AeadInPlace`), `Key::generate` and the prefix via `Generate` (no `OsRng`, no deprecated
      `generate_key`), `seal.rs` nonce via `XNonce::generate()`, `Aead::encrypt` and `Payload` kept.
- [x] `matches_aead_stream_be32` removed with the 0.10 reference, test helpers in
      `blob/tests.rs` switch `as_slice().into()` to `try_into().unwrap()`, and
      `frames_sealed_without_the_header_in_the_aad_are_rejected` seals with a hand-built nonce
      instead of `EncryptorBE32` (I11). Assertions and both KAT constants byte-identical
      (check with `git diff -U0` on the `KAT_` lines and `assert_eq!` lines).
- [x] `seal_matches_xchacha_known_answer`, `KAT_SMALL`, `KAT_TWO_FRAMES_SHA256` and
      `frame_nonce_is_prefix_counter_and_last_flag` pass.
- [x] `layer_rules_hold` green, `rg 'chacha20poly1305|aead::' src --glob '!src/sync/crypto/**'` empty.

Files: `Cargo.toml`, `Cargo.lock`, `src/sync/crypto/mod.rs`, `src/sync/crypto/seal.rs`, `src/sync/crypto/blob/stream.rs`, `src/sync/crypto/blob/mod.rs`, `src/sync/crypto/blob/tests.rs`
Depends on: Stage 43

### Stage 45: #167 final verification

Goal: all gates green, docs match.
Owner: implementer

- [x] `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
      `cargo +1.88 check --all-targets --locked`, `cargo check --target x86_64-pc-windows-msvc`,
      `cargo build --release && ./tests/e2e_test.sh`.
- [x] A v2 blob sealed before Stage 42 (sealed from HEAD of Stage 41 into a temp file) opens after
      Stage 44.
- [x] `stream.rs` about 100 lines or less without tests, ADR status of the section set to
      Accepted (built), memory note facts unchanged, #167 gets a comment with the dependency diff.

Files: `.state/feature-sync-gist/ADR.md`, `.state/feature-sync-gist/PLAN.md`
Depends on: Stage 44

## Pack layer stages

References: ADR "Pack layer (2026-11-09)", rough stages 1 to 11. ADR stage 12 ("Later") is not planned here.

**PR split**: #166 stays open and goes to main only after the whole pack layer is done ("i dont want to merge a half finished #166 with a design that changes"). The working branch is `feature/sync-packs`, created on top of `feature/sync-gist`. #166 has 117 changed files vs main, over the CodeRabbit limit of 100, as such nothing of the pack layer is added to its own diff.
- **PR B** (`feature/sync-packs` into `feature/sync-gist`): Stage 46 (probe file) and Stages 47 to 53, the store and pack layers and the gist store, reachable only through `sync::testing`, no user visible change.
- **PR C** (new branch on `feature/sync-gist` after PR B): Stages 54 to 60, engine and app switch to packs, `agr sync delete` with patterns, `agr sync migrate`, removal of the per-session code, docs.
- **PR D** (new branch on `feature/sync-gist` after PR C): Stages 61 and 62, `agr sync prune`.
- **#166** (`feature/sync-gist` into main): merged only after B, C and D are in it.

Reasons:
1. A half finished #166 with a design that changes is not wanted on main, as such the pack PRs land on the feature branch first.
2. PR B only adds new files next to the old ones, so it is reviewable on its own.
3. Migrate sits in the same PR as the switch: once the engine reads packs, the ~60 per-session gists the user already pushed are invisible to sync, as such the release that switches has to bring `agr sync migrate`.
4. Prune is only needed once dead data piles up, it follows without blocking anyone.

Rough size (re-estimated 2026-11-09 after the review round, union of the `Files:` lines per PR plus the state files): PR B about 35 files, PR C about 80 files (the 13 moved or deleted `backend/` files count too), PR D about 20. C stays under 100 (74 listed files, about 80 with helpers the stages touch on the way), as such no C1 and C2 split. The 100 file CodeRabbit limit applies to the diff of each pack PR against `feature/sync-gist`, the last stage of each PR checks `git diff --stat origin/feature/sync-gist...HEAD` and splits before 100.

**TDD for these stages** (the last round skipped separate RED commits, as such this is spelled out):
- Every `RED commit:` item is its own commit `test(<scope>): ... (RED)` that compiles and fails on the named assertion. Every test in a RED item names its own failing assertion in brackets, not only the first one. Stubs return a wrong value (empty vec, `Ok(())`, `false`), never `todo!()` or `unimplemented!()`, and a compile error is not a RED.
- The `GREEN commit:` item is the next commit `feat(...)` or `fix(...)` and does not touch the RED test except imports.
- The reviewer checks `git log --oneline` for the RED and GREEN pair of every item, a missing RED commit is a blocking finding.
- Sizes: functions about 20 lines, files under 400 lines, tests move into a sibling `tests.rs` once a module passes about 250 lines. Every stage names where a split is expected.

**Probe branches**: Stage 46 decided ADR Open risk 1 on 2026-11-09. Partial clone append works and secret gists read, but the second probe `append_fetch_trigger` showed git fetches the existing parts during every commit and push variant, and with `GIT_NO_LAZY_FETCH=1` the push fails. No recipe exists, as such branch C is the plan.
- **Branch C** (cached clone of the own open pack): kept under `<cache dir>/agr/packs/<id>` with 0700 on unix, at most one pack size locally, fast-forwarded before every append, removed after its seal, on `Moved` and on a writer id or length mismatch, cloned again once if missing.
- **Reads**: `raw_url` plus `Range` is the one read path (verified on secret gists for 8 MiB binary parts), there is no clone fallback. The cached clone is only used for appends.

### Stage 46: Pack probe against real secret gists (live, by hand)

Goal: answer ADR Open risk 1 and the `raw_url` question with real secret gists before any pack code.
Owner: implementer (written in parallel by another agent), run by the user
PR: B

- [ ] `tests/pack_probe_live.rs` written, every test `#[ignore]`, gists deleted on drop, every fact printed as a `PROBE:` line (in progress).
- [ ] Default `cargo test` stays offline, the probe only runs with `--ignored`.
- [ ] The user runs `cargo test --test pack_probe_live -- --ignored --nocapture --test-threads=1` by hand and hands over the `PROBE:` lines.
- [x] First live run (2026-11-09) recorded in the ADR: partial clone push and `raw_url` plus Range pass, the existing blob was fetched during the append.
- [x] Second probe `append_fetch_trigger` written (compile only) and run by the user (2026-11-09, every commit and push fetched `data.000`): `cargo test --test pack_probe_live append_fetch_trigger -- --ignored --nocapture --test-threads=1`. Every `PROBE: fetch.<step>` line says which step fetches `data.000`.
- [x] ADR Open risk 1 is closed as branch C (no recipe exists).
- [x] Third probe `updated_at_moves_on_git_push` written (compile only), run by the user (2026-11-09, both PASS): `cargo test --test pack_probe_live updated_at_moves_on_git_push -- --ignored --nocapture --test-threads=1`. `PROBE: updated_at.api` and `PROBE: updated_at.listing` decide the index cache key of Stage 52 (ADR "Index cache (verified 2026-11-09)").

Files: `tests/pack_probe_live.rs`, `.state/feature-sync-gist/ADR.md`
Depends on: Stage 45

Considerations: no RED and GREEN here, it is a measurement. Stage 53 picks its branch from the second probe, Stages 47 to 52 do not wait for it. `tests/pack_probe_live.rs` stays in the tree as an ignored test (P11), it documents the verified facts and can be re-run when GitHub changes, Stage 59 does not delete it.

### Stage 47: Store trait, memory and directory stores, layer rules

Goal: `PackStore` as in ADR "Store trait" with `MemoryStore` and `DirStore`, only reachable through `sync::testing`.
Owner: implementer
PR: B

- [x] RED commit: `violations_flags_pack_naming_engine` [`assert_eq!(v.len(), 1)`, gives 0] and `violations_flags_store_naming_crypto` [`assert_eq!(v.len(), 1)`, gives 0] in `src/sync/mod.rs`.
- [x] GREEN commit: this stage extends `layer_rules_hold` for `pack/` and `store/`. New `RULES` rows: `pack/` must not name `crate::sync::app`, `crate::sync::engine`, `crate::sync::store::memory`, `crate::sync::store::dir`, `crate::sync::store::gist`, and `store/` must not name `crate::sync::crypto`, `crate::sync::engine`, `crate::sync::pack`, `crate::sync::app`, `chacha20poly1305`, `sha2::`, `aead::`. `layer_rules_hold` green on the tree.
- [x] RED commit: `store_error_messages_name_the_next_step` in `store/mod.rs` [`assert_eq!(StoreError::Moved.to_string(), "changed on the remote, run again")`, the stub `Display` writes an empty string], plus `RateLimited` [`assert!(msg.contains("run again later"))`] and `NotFound` [`assert!(msg.contains("not found"))`].
- [x] GREEN commit: `StoreError` (`NotInstalled`, `NotLoggedIn`, `MissingScope`, `Unreachable`, `RateLimited`, `NotFound`, `Moved(String)`, `Other`) with `Display`, `PackInfo { id, tag, len, updated_at }`, `Listing { packs, legacy }`, `Segment`. `Moved` carries the index text at the current remote head, its `Display` never prints it. `BlobSource` is re-exported from `backend/mod.rs` until Stage 59 moves it.
- [x] RED commit, shared `contract(store: &dyn PackStore)` (`store/contract.rs`, cfg test), run for `MemoryStore`:
  - `append_refuses_wrong_expected_len` [`assert!(matches!(err, StoreError::Moved(_)))` for `expected_len` 10 on a new pack, the stub returns `Ok(())`]
  - `append_refuses_other_expected_index` [`assert!(matches!(err, StoreError::Moved(_)))` when `expected_index` is not the current index text, length equal]
  - `moved_carries_the_current_index` [`assert_eq!(err, StoreError::Moved(current))` where `current` is the index another append just wrote, the stub returns `Moved(String::new())`]
  - `zero_segment_append_replaces_only_the_index` [`assert_eq!(store.read_index(&id)?, new_index)` and `assert_eq!(info.len, old_len)`]
  - `append_is_all_or_nothing` [`assert_eq!(info.len, old_len)` and `assert_eq!(store.read_index(&id)?, old_index)` after a failing segment writer]
  - `read_returns_the_exact_range` [`assert_eq!(bytes, joined[a..b])` for a range across two appends]
  - `create_then_list_shows_len_zero_and_tag` [`assert_eq!((info.len, info.tag.as_str()), (0, tag))`, `assert!(listing.legacy.is_empty())`]
  - `delete_then_read_index_is_not_found` [`assert!(matches!(err, StoreError::NotFound))`]
- [x] GREEN commit: `MemoryStore` passes the contract. Test hooks: `fail_read_index(id)` (used by the abort tests of Stages 51 and 61) and `with_slots(n)` (file slots for `fits`).
- [x] Trait method `fn fits(&self, info: &PackInfo, lens: &[u64]) -> bool`, default `true`, the gist store overrides it in Stage 52 for the 250 file cap (user approval relayed by the main session, no open question). RED commit: `memory_fits_false_when_slots_run_out` [`assert!(!store.fits(&info, &[1]))` after two appends on `with_slots(2)`, the stub returns `true`]. GREEN commit: `MemoryStore::fits` counts slots.
- [x] RED commit: the contract runs for `DirStore` too, plus `dir_store_crash_after_parts_before_manifest_recovers` (test-only fault hook between part files and the manifest rename) [`assert_eq!(store.list()?.packs[0].len, old_len)` after the crash, then `assert!(store.append(&id, old_len, &old_index, &segs, &idx).is_ok())`, the stub sums part files and gives `Moved` forever].
- [x] GREEN commit: `DirStore` writes new part files first, then one `manifest` (tag, `len`, part list, index) via `manifest.tmp` renamed once. `len` comes from the manifest only, unlisted part files are deleted by the next append.

Files: `src/sync/mod.rs`, `src/sync/store/mod.rs`, `src/sync/store/memory.rs`, `src/sync/store/dir.rs`, `src/sync/store/contract.rs`
Depends on: Stage 45

Considerations: `store/mod.rs` is trait and types only (about 120 lines), `memory.rs` and `dir.rs` under 250 each. Exports in `sync::testing` keep clippy free of dead code warnings until PR C wires it.

### Stage 48: Pack index and seal rules

Goal: `pack/index.rs` with the index JSON v3 sealed under AAD `agr:pack-index:` plus the pack uuid as base64url text, and the pure seal rules in `pack/rules.rs`.
Owner: implementer
PR: B

- [ ] RED commit: `index_round_trips_through_seal` [`assert_eq!(open_index(key, uuid, &seal_index(key, &idx))?, idx)`, the stub returns an empty index].
- [ ] RED commit:
  - `index_with_other_key_is_other_key` [`assert!(matches!(err, IndexError::OtherKey))`]
  - `index_with_v_4_is_unknown_version` [`assert!(matches!(err, IndexError::UnknownVersion(4)))`]
  - `index_text_is_base64url` [`assert!(!text.contains(['=', '+', '/']))`]
  - `index_from_another_pack_is_rejected` [`assert!(matches!(open_index(key, other_uuid, &text), Err(IndexError::OtherPack)))`, the AAD binds the pack uuid and the stub ignores it]
  - `two_seals_of_one_index_differ` [`assert_ne!(seal_index(k, &i), seal_index(k, &i))`, fresh nonce, the `expected_index` guard relies on it]
  - `pack_uuid_is_26_base32_chars` [`assert_eq!(new_pack_uuid().len(), 26)`]
- [ ] GREEN commit: `Index { v, pack, writer, sealed, len, entries }`, `Entry { path, gen, sha256, size, offset, length, deleted, at }`, `seal_index`, `open_index`, `new_pack_uuid` on the existing one-shot `seal`.
- [ ] RED commit, table tests in `rules.rs`:
  - `seal_at_target` [`assert!(should_seal(target, target))`, the stub returns `false`, plus `assert!(!should_seal(target, target - 1))`]
  - `big_session_gets_own_pack` [`assert_eq!(place(target, target + 1), Place::OwnPack)`]
  - `index_over_768_kib_seals_early` [`assert!(should_seal_index(768 * 1024 + 1))`]
  - `pack_size_mib_range_16_to_512` [`assert!(pack_size(8).is_err())` and `assert!(pack_size(1024).is_err())`, `assert!(pack_size(16).is_ok())`]
- [ ] GREEN commit: `rules.rs` pure functions, no store and no IO.

Files: `src/sync/mod.rs`, `src/sync/pack/mod.rs`, `src/sync/pack/index.rs`, `src/sync/pack/rules.rs`
Depends on: Stage 47

Considerations: `index.rs` about 220 lines with tests, `rules.rs` under 150. The 768 KiB check measures the encoded text, not the JSON.

### Stage 49: Pack stream, segments and range read

Goal: `pack/stream.rs` turns session files into `Segment`s (blob v2 with path and gen in the AAD) and opens a range from `read(id, offset, len)` back into the session.
Owner: implementer
PR: B

- [ ] RED commit: `segment_len_equals_written_bytes` for plain sizes 0, 1, 64 KiB, 64 KiB + 1 [`assert_eq!(seg.len, written)`, stub `len` of 0].
- [ ] RED commit:
  - `range_read_opens_the_second_session` [`assert_eq!(sha256(opened), sha256(file))` after two appends to a `MemoryStore`]
  - `blob_under_other_path_or_gen_fails` [`assert!(matches!(err, CryptoError::Authentication))`]
  - `truncated_range_fails` [`assert!(open_range(.., len - 1).is_err())` and `assert!(out.is_empty())`, no partial plaintext]
- [ ] GREEN commit: `segment(path, gen, file)`, `open_range`. The sealed length comes from a pure `sealed_len(size)` in `crypto/blob`, added there when it does not exist yet.

Files: `src/sync/pack/mod.rs`, `src/sync/pack/stream.rs`, `src/sync/crypto/blob/mod.rs`
Depends on: Stage 48

Considerations: `stream.rs` under 200 lines. `crypto/blob/mod.rs` is 186 lines, `sealed_len` stays a few lines, KAT constants untouched.

### Stage 50: Merge

Goal: `pack/merge.rs`, the five merge rules of the ADR as pure functions over opened indexes.
Owner: implementer
PR: B

- [ ] RED commit: `highest_gen_wins` [`assert_eq!(view.get("a").unwrap().gen, 4)`, the stub keeps the first entry seen].
- [ ] RED commit:
  - `same_hash_twice_is_one_session_two_places` [`assert_eq!(view.get("a").unwrap().places.len(), 2)`]
  - `different_hash_same_top_gen_is_fork` [`assert_eq!(view.forks, vec!["a"])`]
  - `fork_below_top_gen_is_ignored` [`assert!(view.forks.is_empty())`]
  - `tombstone_on_top_is_deleted_with_its_gen` [`assert_eq!(view.get("a").unwrap().top, Top::Deleted { gen: 5 })`, not absent, the next push uses 6]
  - `other_key_pack_is_reported_never_merged` [`assert_eq!(view.rejected, vec![(id, Rejected::OtherKey)])` and `assert!(view.get("x").is_none())`]
  - `unknown_version_pack_is_reported` [`assert_eq!(view.rejected[0].1, Rejected::UnknownVersion(4))`]
  - `index_of_other_pack_is_reported` [`assert_eq!(view.rejected[0].1, Rejected::OtherPack)`]
  - `dead_bytes_per_pack` [`assert_eq!(view.dead_bytes[&id], old_gen_length)`, live vs dead length per pack, a top-gen tombstone counts as live, used by status and prune]
  - `duplicated_pack_is_merged_and_reported` (ADR merge rule 6) [`assert_eq!(view.duplicates, vec![("a", "b")])` for packs `a` and `b` with one tag and the same index, `assert_eq!(view.get("x").unwrap().places.len(), 2)` and a tombstone only in that tag stays `is_deleted()`, the stub drops both packs]
  - `keeper_is_the_place_in_the_highest_pack_id` [`assert_eq!(view.get("a").unwrap().keeper().pack, "b")` for places in packs `a` and `b`, the other place counts as dead bytes]
- [ ] GREEN commit: `merge(packs)` returning a `View` with `places`, `keeper`, `forks`, `rejected`, `duplicates` and `dead_bytes`. `View::is_frozen(id)` is true for both ids of a duplicate, push, prune and `--seal-orphans` never write or delete a frozen pack.

Files: `src/sync/pack/mod.rs`, `src/sync/pack/merge.rs`, `src/sync/pack/merge_tests.rs`
Depends on: Stage 49

Considerations: pure, no store, `merge.rs` under 250 lines, the tests in `merge_tests.rs`.

### Stage 51: Repo facade over any store

Goal: `pack::Repo` with `open`, `view`, `push_batch`, `read_session`, `tombstone` and the re-read after the last append, tested on `MemoryStore` only.
Owner: implementer
PR: B

- [ ] RED commit: `push_batch_appends_to_own_open_pack` [`assert_eq!(store.list()?.packs.len(), 1)` after two pushes on one writer, the stub creates a pack per push].
- [ ] RED commit:
  - `seal_then_next_push_creates_a_pack` [`assert_eq!(store.list()?.packs.len(), 2)`]
  - `big_session_gets_own_sealed_pack_and_open_pack_stays_open` [`assert!(own.sealed)` and `assert_eq!(repo.open_pack(), Some(old_id))`]
  - `open_pack_found_by_writer_when_local_id_missing` [`assert_eq!(repo.open_pack(), Some(id))`]
  - `index_of_other_writer_or_sealed_is_moved` [`assert!(matches!(err, StoreError::Moved(_)))`]
  - `own_open_pack_duplicated_starts_a_new_pack` [`assert_ne!(repo.open_pack(), Some(a))` and `assert_eq!(store.list()?.packs.len(), 3)` after a push when the own open pack `a` has a copy `b`, the stub appends to `a`]
  - `push_of_440_mib_is_six_sealed_and_one_open_pack` (P1, scaled by 1/1024: 721 sessions summing to 440 KiB against a 64 KiB target, the target is an argument) [`assert_eq!((sealed, open), (6, 1))`] and `pack_tag_is_the_index_uuid` [`assert_eq!(info.tag, index.pack)`]
- [ ] RED commit (P3, `fits`):
  - `seal_when_fits_is_false` [`assert!(old.sealed)` and `assert_eq!(store.list()?.packs.len(), 2)` on `MemoryStore::with_slots(3)`, the stub ignores `fits`]
  - `no_file_slot_left_seals_with_zero_byte_append` [`assert_eq!(old_info.len, len_before)` and `assert!(old_index.sealed)`]
- [ ] RED commit: `two_writers_on_one_id_retry_once` (ADR Open risk 3, mandatory), two cases:
  - data case [`assert!(matches!(first, Err(StoreError::Moved(_))))` for the second writer, then after the retry `assert_eq!(view.len(), 2)`, a second `Moved` gives `assert_eq!(err.to_string(), "changed on the remote, run again")`]
  - `retry_uses_the_index_from_moved` [`assert_eq!(store.read_index_calls(id), 0)` during the retry and the retry's `expected_index` equals the `Moved` text, the stub re-reads through `read_index`]
  - tombstone case: A tombstones s, B tombstones t on an index read before A's push [`assert!(view.get("s").unwrap().is_deleted() && view.get("t").unwrap().is_deleted())`, the stub guards on length only and s comes back]
- [ ] RED commit: `read_pack_takes_the_key_as_argument` (P9, room for `agr share`): `pack::read_pack(store, id, &key)` opens one pack sealed with a test key that is not the sync key, without the merge and without any global or app key [`assert_eq!(sessions.len(), 2)`, the stub returns an empty vec].
- [ ] RED commit:
  - `read_session_tries_next_place_on_not_found` [`assert_eq!(sha, want)` with the first place deleted]
  - `read_session_checks_sha256_after_open` [`assert!(matches!(err, ReadError::HashMismatch))` and `assert!(!target.exists())`, a range that opens under the right AAD but whose plaintext hash differs from the index is never written]
  - `tombstone_appends_deleted_at_gen_plus_one` [`assert_eq!((e.gen, e.deleted), (4, true))`]
  - `fork_created_by_push_is_reported_by_the_re_read` [`assert_eq!(report.forks, vec!["x"])`]
  - `open_fails_when_any_index_read_fails` [`assert!(matches!(Repo::open(..), Err(RepoError::PartialView(_))))` with `fail_read_index` on one of three packs, the stub skips the pack]
- [ ] GREEN commit per RED commit above.

Files: `src/sync/mod.rs`, `src/sync/pack/mod.rs`, `src/sync/pack/repo/mod.rs`, `src/sync/pack/repo/push.rs`, `src/sync/pack/repo/tests.rs`
Depends on: Stage 50

Considerations: split expected from the start, `repo/mod.rs` (open, view, read) and `repo/push.rs` (push_batch, retry, tombstone), each under 300 lines, `repo/tests.rs` splits into `tests_push.rs` once it passes 400. The writer id and the target size come in as arguments, the app owns them (Stage 54).

### Stage 52: Gist store, layout, read side and index cache

Goal: `store/gist` lists `agr:v2` gists, reads the index through `gh api` with a local index cache, creates a pack gist, reads a range via `raw_url` plus `Range` (the one read path, no clone fallback), and cuts 8 MiB parts at session boundaries. It reuses the runner, pace, progress and `Fake` of `backend/gist/` until Stage 59 moves them.
Owner: implementer
PR: B

- [x] Visibility and error changes in `backend/gist/` (no behaviour change, a commit `refactor(sync): ...` before the first RED): `mod progress` and `writing_percent`, `receiving_percent` become `pub(crate)`, `Pacer`, `SystemClock`, `MIN_GAP` and `runner::Lines` become `pub(crate)`, `Fake`, `ok`, `fail` in `fake.rs` become `pub(crate)` under `cfg(test)`. `Clock::unix_secs` returns `Result<u64, ClockError>` (a small error in `pace.rs`) instead of `BackendError`, `BackendError` and `StoreError` both get `From<ClockError>`. The existing `pace.rs` tests stay green.
- [x] RED commit: `parts_cut_at_session_boundaries` [`assert_eq!(parts, vec![7 * MIB, 2 * MIB])` for lens 3, 4 and 2 MiB, the stub gives one part per session].
- [x] RED commit:
  - `session_over_8_mib_gets_own_parts` [`assert_eq!(parts, vec![8 * MIB, 8 * MIB, 4 * MIB])` for 20 MiB]
  - `append_numbers_continue_after_last_part` [`assert_eq!(names[0], "data.003")`]
  - `gap_or_order_in_parts_is_an_error` [`assert!(check_parts(&["data.000", "data.002"]).is_err())`, port of the `parts.rs` tests]
  - `fits_refuses_over_250_files` [`assert!(!store.fits(&info_249_files, &[9 * MIB]))`]
  - `fits_false_when_no_file_slot_left` [`assert!(!store.fits(&info_250_files, &[1]))`]
- [x] GREEN commit: `layout.rs`.
- [x] RED commit (Fake runner):
  - `list_keeps_only_agr_v2_over_pages` [`assert_eq!(ids, vec!["a", "c"])` over two pages]
  - `list_passes_agr_v1_descriptions_as_legacy` [`assert_eq!(listing.legacy, vec!["agr:v1 tok"])`, `agr:pending` and foreign gists are in neither list, the stub drops them]
  - `truncated_index_is_other_not_half_an_index` [`assert!(matches!(err, StoreError::Other(_)))`]
  - `create_sends_only_the_index` [`assert_eq!(files, vec!["index"])`, no `.keep`]
  - `pack_description_is_agr_v2_and_uuid_only` [`assert_eq!(desc, format!("agr:v2 {tag}"))`]
  - `read_range_requests_only_covering_parts` [`assert_eq!(range_calls, vec!["data.001 bytes=..."])`, no clone in the calls]
  - `failed_raw_url_read_is_a_store_error_no_clone` [`assert!(matches!(err, StoreError::Unreachable))` and `assert!(!calls.iter().any(|c| c.contains("clone")))`]
- [x] GREEN commit: `api.rs` and `read.rs`.
- [x] Probe dependency: the key comes from `updated_at_moves_on_git_push` (Stage 46). Both lines PASSED on 2026-11-09: the tests below as written, the fallback is not needed. Either line FAILS: the fallback of ADR "Index cache (pending probe)", the two `updated_at` tests become `sealed_entry_is_cached_forever_by_id` [`assert_eq!(api_calls_for(id), 0)` with a changed `updated_at`] and `open_pack_is_read_every_time_and_keyed_on_history_version` [`assert_eq!(api_calls_for(id), 2)` over two opens, the entry key is `history[0].version`], plus `gone_id_drops_its_entry` [`assert!(cache.get(id).is_none())`].
- [ ] RED commit, index cache (`<user cache dir>/agr/index-cache.json`, keyed by gist id plus `updated_at`):
  - `sealed_index_cache_hit_skips_the_api` [`assert_eq!(api_calls_for(id), 0)` on the second `read_index` with the same `updated_at`, the stub always calls the API]
  - `changed_updated_at_refetches` [`assert_eq!(api_calls_for(id), 1)`]
  - `cache_survives_corrupt_file` [`assert_eq!(store.read_index(id)?, text)` with garbage in the cache file, the file is rewritten valid]
  - `cache_entry_dropped_on_moved` [`assert!(cache.get(id).is_none())`]
- [ ] GREEN commit: `index_cache.rs`, temp plus rename, sealed text only.
- [ ] Probe dependency: Stage 46 verified `raw_url` with `Range` (206, correct bytes) on secret gists, so no branch is needed here.

Files: `src/sync/store/mod.rs`, `src/sync/store/gist/mod.rs`, `src/sync/store/gist/layout.rs`, `src/sync/store/gist/api.rs`, `src/sync/store/gist/read.rs`, `src/sync/store/gist/index_cache.rs`, `src/sync/store/gist/tests.rs`, `src/sync/backend/gist/mod.rs`, `src/sync/backend/gist/pace.rs`, `src/sync/backend/gist/progress.rs`, `src/sync/backend/gist/runner.rs`, `src/sync/backend/gist/fake.rs`, `src/sync/backend/mod.rs`
Depends on: Stage 47

Considerations: `layout.rs` pure (about 150 lines), `mod.rs` only the `PackStore` impl dispatching (about 100), `index_cache.rs` under 150, tests in `tests.rs`. Runs in parallel with Stages 48 to 51, no shared files.

### Stage 53: Gist store append, PR B gates

Goal: `append` per ADR "Append and seal" steps 2, 3 (store side), 4 and 6 (steps 3 pack side and 5 are the pack layer), in branch C (cached clone of the own open pack), picked by the second probe `append_fetch_trigger` on 2026-11-09.
Owner: implementer
PR: B

- [ ] RED commit: `tree_len_not_expected_is_moved` [`assert_eq!(err, StoreError::Moved(head_index))` with `head_index` the `index` blob at `HEAD` of the fast-forwarded clone, when the sum of the part sizes in the tree differs from `expected_len`, the stub pushes anyway].
- [ ] RED commit: `zero_byte_append_on_stale_index_is_moved` [`assert_eq!(err, StoreError::Moved(head_index))` and `assert!(!calls.iter().any(|c| c.contains("push")))`: the `index` blob at `HEAD` of the cached clone after the fast-forward differs from `expected_index`, the length is equal and there are no segments, the stub checks the length only]. The index for the check is read from the clone, never from the API or the index cache.
- [ ] RED commit:
  - `rejected_push_is_moved_with_the_fetched_index` [`assert_eq!(err, StoreError::Moved(remote_index))` on non-fast-forward stderr, read from `FETCH_HEAD:index` after one `git fetch`, `assert!(!args.contains("--force"))`, a failing fetch gives its own error and no `Moved`]
  - `append_is_one_commit_with_parts_and_index` [`assert_eq!(commits, 1)` and `assert_eq!(changed, vec!["data.002", "index"])`]
  - `pushes_to_one_gist_are_paced_10s` [`assert_eq!(clock.slept(), vec![Duration::from_secs(10)])` for a second append on the same id, other mutating calls keep 1 s]
  - `pacing_holds_across_processes` [`assert_eq!(clock.slept(), vec![Duration::from_secs(7)])`: two `GistStore` values with fresh in-process pacers share one `pace.toml`, the second appends 3 s after the first and waits the rest, the stub keeps the time in memory only]
  - `progress_percent_belongs_to_the_append` [`assert_eq!(sink.last(), Some(100))` for the append, no percent from the fetch]
- [ ] RED commit (branch C, cached clone of the own open pack, Stage 46 second probe ruled out partial clone appends):
  - `cached_clone_is_reused_across_appends` [`assert_eq!(count(&calls, "clone"), 1)`, second append runs `git fetch` plus `merge --ff-only`]
  - `cached_clone_removed_on_seal` [`assert!(!cache_dir.join(id).exists())`]
  - `missing_cached_clone_is_cloned_once` [`assert_eq!(count(&calls, "clone"), 1)` and a second failure `assert!(matches!(err, StoreError::Other(_)))`, no loop]
  - `cached_clone_with_other_index_is_rejected_and_recloned` [`assert!(matches!(err, StoreError::Moved(_)))` and `assert!(!cache_dir.join(id).exists())`]
  - `cached_clone_with_stale_len_is_rejected_and_recloned` [as above]
  - `cached_clone_dropped_on_moved` [`assert!(!cache_dir.join(id).exists())`]
  - `cache_path_is_inside_user_cache_dir` [`assert_eq!(path, cache_dir.join("agr/packs").join(id))`]
  - `cache_dir_is_0700_on_unix` (`#[cfg(unix)]`) [`assert_eq!(mode & 0o777, 0o700)`]
- [ ] RED commit: `gist_store_passes_the_contract` (P2), the Stage 47 `contract` run against `GistStore` over a `Fake` runner that keeps a small in-memory gist remote [`contract` panics on `append_refuses_other_expected_index` with the stub].
- [ ] GREEN commit: full clone into `<cache dir>/agr/packs/<id>` on first use, `git fetch` plus fast-forward only after that, the `expected_index` and length check on the clone, the push uploads only the new parts, the last push time per gist in `<cache dir>/agr/pace.toml`.
- [ ] `tests/pack_gist_live.rs` (ignored): create, two appends, one zero-byte append, read both ranges, delete. The user runs it once.
- [ ] PR B gates: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo +1.88 check --all-targets --locked`, `cargo check --target x86_64-pc-windows-msvc`, `git diff --stat origin/feature/sync-gist...HEAD` under 100 files, `wc -l` of every new file under 400.

Files: `src/sync/store/gist/mod.rs`, `src/sync/store/gist/append.rs`, `src/sync/store/gist/cache.rs`, `src/sync/store/gist/last_push.rs`, `src/sync/store/gist/fake_remote.rs`, `src/sync/store/gist/tests.rs`, `src/sync/store/contract.rs`, `tests/pack_gist_live.rs`
Depends on: Stage 46, Stage 51, Stage 52

Considerations: the one retry lives in `Repo` (Stage 51), the store only reports `Moved`. `append.rs` under 250 lines, the cache handling goes into `cache.rs`, the persisted pace into `last_push.rs` (under 80 lines, read and written only while the caller holds the local sync lock, see Stage 55), the in-memory gist remote of the contract run into `fake_remote.rs`.

### Stage 54: Engine and app on the Repo, local index v2

Goal: the engine plans against the merged view and writes through `Repo`, the app builds `Repo` over the gist store, the engine no longer calls `SyncBackend`.
Owner: implementer
PR: C

- [ ] RED commit: `push_of_three_sessions_is_one_pack` in `sync_engine_test.rs` on `MemoryStore` [`assert_eq!(store.list()?.packs.len(), 1)`]. The commit carries the new `Engine` constructor with a stub body returning an empty report, so it compiles.
- [ ] RED commit:
  - `push_on_v1_index_requires_migrate_first` [`assert!(err.to_string().contains("run `agr sync migrate` first"))` and `assert_eq!(appends, 0)`, for push, pull and delete, the stub reads v1 as empty]
  - `open_pack_saved_after_push_and_cleared_after_seal` [`assert_eq!(index.open_pack, Some(id))`, then `assert_eq!(index.open_pack, None)`]
  - `delete_is_a_tombstone` [`assert!(view.get(p).unwrap().is_deleted())`]
  - `rollback_below_local_gen_is_refused` (port) [`assert!(report.refused.contains(p))` and the local file is unchanged]
  - `overwrite_resolves_fork_with_gen_plus_one` [`assert_eq!(view.get(p).unwrap().gen, 5)` and `assert!(view.forks.is_empty())`]
- [ ] RED commit, `Remote::Deleted { gen }` in `engine/plan.rs` (ADR "Merge", today `(Local::Hash(_), Remote::Absent)` is `Push`):
  - `deleted_remote_is_not_pushed_back` [`assert_eq!(decide(local == last, Remote::Deleted { gen: 5 }), Decision::Skip(Reason::DeletedOnRemote))` and the reason text contains "agr sync push --overwrite", the stub maps `Deleted` to `Absent` and gives `Push`]
  - `local_edit_after_remote_delete_is_a_fork` [`assert_eq!(decide(local != last, Deleted { gen: 5 }), Decision::Fork)`]
  - `overwrite_restores_a_deleted_session` (engine run on `MemoryStore`) [`assert_eq!(view.get(p).unwrap().gen, 6)` and `assert!(!view.get(p).unwrap().is_deleted())`]
  - `local_absent_remote_deleted_is_in_sync` [`assert!(matches!(d, Decision::InSync { .. }))` and the index entry is dropped]
- [ ] RED commit: `fresh_machine_with_v1_gists_on_remote_requires_migrate` in `sync_multi_machine_test.rs` (ADR "Fresh machine refuses too"): machine B has no local index, the store lists one `agr:v1` gist sealed with our key [`assert!(err.to_string().contains("run `agr sync migrate` first"))` and `assert_eq!(appends, 0)` for push, pull and delete, the stub reads the missing index as empty v2 and pushes gen 1]. Same test, two more cases: a deleted local index on machine A refuses the same way, and an `agr:v1` gist of another key does not block [`assert!(push.is_ok())`]. `Repo::open_for_migrate` skips the guard [`assert!(open_for_migrate(..).is_ok())`].
- [ ] RED commit: `fork_is_reported_right_after_push` in `sync_multi_machine_test.rs` (two machines, two writer ids) [`assert_eq!(report.forks, vec!["claude/x.cast.gz"])`].
- [ ] RED commit:
  - `pack_size_mib_defaults_to_64` in `config/types.rs` [`assert_eq!(cfg.sync.pack_size_mib, 64)`]
  - `pack_size_mib_out_of_range_is_a_clear_error` (P4) [`assert!(err.to_string().contains("pack_size_mib must be between 16 and 512"))` for 8 and 1024]
  - `writer_id_created_once_in_state_toml` in `config/state.rs` [`assert_eq!(first, second)` over two loads]
  - `new_writer_id_after_state_loss` (P5) in `sync_multi_machine_test.rs` [`assert_ne!(old_writer, new_writer)`, and after a push with the new id `assert!(view.get(old_path).is_some())` and `assert_eq!(store.list()?.packs.len(), 2)`, the old open pack stays readable and merged]
- [ ] Guard test commit `test(config): ...` (P14, existing behaviour, passes on arrival, the reviewer checks it fails when the default is flipped by hand): `sync_defaults_are_gist_and_auto_push_off` in `config/types.rs` [`assert_eq!((kind, auto), (Gist, false))`], re-checked once `pack_size_mib` is added.
- [ ] RED commit: `missing_gh_or_git_breaks_nothing_outside_sync` in `cli_sync_test.rs` (P14): with `PATH` stripped of `gh` and `git`, `agr list` and `agr status` exit 0 with their normal output, `agr sync push` exits non-zero with the one line "gh not found, the gist backend needs gh and git" and the storage dir is unchanged [`assert!(!push.status.success())` and `assert_eq!(before, after)`]. Guard test commit as above (existing behaviour on the per-session code, kept true while `setup.rs` builds the `Repo`), it turns RED if the store is opened outside sync commands.
- [ ] RED commit: `violations_flags_engine_naming_gist_store` [`assert_eq!(v.len(), 1)`, gives 0], GREEN adds `RULES` rows so `engine/` must not name `crate::sync::store::gist`, `crate::sync::store::memory`, `crate::sync::store::dir`.
- [ ] GREEN commit per RED commit above. `AGR_SYNC_TEST_BACKEND_DIR` now points a `DirStore` at the dir, so `tests/e2e_test.sh` keeps its variable.

Files: `src/sync/mod.rs`, `src/sync/engine/run.rs`, `src/sync/engine/remote.rs`, `src/sync/engine/push.rs`, `src/sync/engine/pull.rs`, `src/sync/engine/plan.rs`, `src/sync/engine/index.rs`, `src/sync/engine/item.rs`, `src/sync/engine/scan.rs`, `src/sync/app/setup.rs`, `src/commands/sync/run.rs`, `src/config/types.rs`, `src/config/state.rs`, `tests/integration/sync_engine_test.rs`, `tests/integration/sync_multi_machine_test.rs`, `tests/integration/sync_index_fields_test.rs`, `tests/integration/helpers/sync_net.rs`, `tests/integration/cli_sync_test.rs`
Depends on: Stage 53

Considerations: the biggest stage, the reviewer-pair runs after every RED and GREEN pair. `run.rs` is 369 lines today, the merged view lookups split out into `engine/remote.rs`. `plan.rs` gains the `Deleted` rows only (about 30 lines). `index.rs` loses `remote_id`, `token`, `pending` only in Stage 59, here it gains `version` 2 and `open_pack`, the v1 to v2 conversion itself is Stage 58.

### Stage 55: Auto push on packs

Goal: one drain round is one append, queue and drain stay as they are.
Owner: implementer
PR: C

- [ ] RED commit: `ten_queued_sessions_are_one_append` in `sync_queue_test.rs` [`assert_eq!(appends, 1)`, the per session drain gives 10].
- [ ] RED commit:
  - `burst_during_drain_lands_in_the_next_round_together` [`assert_eq!(appends, 2)`]
  - `single_recording_is_pushed_without_waiting` [`assert_eq!(appends, 1)` right after one recording, no local buffering]
  - `auto_push_on_v1_index_does_nothing_and_hints_migrate` [`assert_eq!(appends, 0)` and the queue entry stays]
  - `auto_push_on_fresh_machine_with_v1_gists_does_nothing` [`assert_eq!(appends, 0)` and `assert_eq!(queue.len(), 1)` with no local index and one `agr:v1` gist of our key on the store, the stub drains and pushes gen 1]
- [ ] RED commit: `n_parallel_recordings_pushed_exactly_once` (P13): 8 threads finish a recording each while a drain runs on a `MemoryStore`, afterwards the merged view has all 8 paths, every path has exactly one entry at gen 1 and the queue is empty [`assert_eq!(view.len(), 8)`, the stub drain takes the queue once and drops what arrives during the round].
- [ ] GREEN commit.
- [ ] Pacing across processes: the drain already runs under `SyncLock` (`engine/queue.rs`), so only one auto push process appends at a time, and the persisted last push time of Stage 53 (`last_push.rs`) makes the next process wait the rest of the 10 s. No new code here, `pacing_holds_across_processes` (Stage 53) covers it.
- [ ] Probe dependency: no code difference here. Cost per round is a fetch of the cached clone (branch C) plus the listing with index cache hits (Stage 52).

Files: `src/sync/app/auto/mod.rs`, `src/sync/app/auto/child.rs`, `src/sync/engine/queue.rs`, `src/recording/auto_push.rs`, `tests/integration/sync_queue_test.rs`
Depends on: Stage 54

Considerations: `child.rs` 317 and `auto/mod.rs` 330 lines today, new logic goes into the engine batch call, not into these files.

### Stage 56: Status and output

Goal: status shows forks with the next step, `OtherKey` per pack, the prune hint, and the live line per append, without any clone and with the index cache.
Owner: implementer
PR: C

- [ ] RED commit: `status_shows_fork_with_next_step` [`assert!(out.contains("agr sync push --overwrite"))`].
- [ ] RED commit:
  - `status_shows_other_key_per_pack` [`assert!(out.contains("other key"))` once per such pack]
  - `status_shows_deleted_on_remote_with_restore_hint` [`assert!(out.contains("deleted on the remote"))`]
  - `prune_hint_at_21_percent_dead_not_at_19` [`assert!(out21.contains("agr sync prune"))` and `assert!(!out19.contains("agr sync prune"))`]
  - `live_line_per_append` [`assert_eq!(live_lines, appends)`]
  - `status_needs_no_clone` [`assert!(!calls.iter().any(|c| c.contains("clone")))`, one API call per pack on a cold cache, zero for sealed packs on a warm one]
  - `status_on_v1_index_hints_migrate` [`assert!(out.contains("agr sync migrate"))`]
  - `status_aborts_on_partial_index_view` [`assert!(!st.success())` and `assert!(out.contains("could not read"))`]
- [ ] RED commit: `status_compact_and_full_list_with_v` (P12) in `cli_sync_test.rs`, 30 sessions on a `DirStore` [`assert!(lines(default) < 10)` and `assert_eq!(listed(verbose), 30)`, the stub always prints the full list].
- [ ] RED commit: `push_and_pull_print_compact_summary_and_full_list_with_v` (P12) [`assert!(lines(default) < 10)` and `assert_eq!(listed(verbose), 30)`].
- [ ] RED commit: `loading_and_live_line_only_on_a_tty` (P12) for push and pull through `format/live.rs` with a fake tty flag [`assert!(tty_out.starts_with(LOADING))` and `assert!(!plain_out.contains(LOADING))`, `NO_COLOR` gives `assert!(!out.contains('\x1b'))`, no `gh` token text in any line].
- [ ] GREEN commit. `Duplicate` leaves the public exports, `Fork` replaces it.
- [ ] Delete, migrate and prune get the same two output tests in Stages 57, 58 and 62, where their commands are built.

Files: `src/sync/mod.rs`, `src/sync/app/format/mod.rs`, `src/sync/app/format/status.rs`, `src/sync/app/format/packs.rs`, `src/sync/app/format/summary.rs`, `src/sync/app/format/live.rs`, `src/commands/sync/live.rs`, `tests/integration/cli_sync_test.rs`
Depends on: Stage 55

Considerations: `status.rs` is 338 lines, the per pack lines go into the new `format/packs.rs`.

### Stage 57: agr sync delete with patterns

Goal: P10, `agr sync delete <pattern>...` and `--all` on the pack layer, collect first, two confirmations for big deletes, a tombstone per session (ADR "Pattern delete").
Owner: implementer
PR: C

- [ ] RED commit: `star_and_question_mark_match_remote_sessions_only` in the pure `engine/select.rs`, patterns `claude/*` and `codex/a?.cast.gz` against `<agent>/<name>` of the merged view, a session that exists only locally never matches [`assert_eq!(hits, vec!["claude/a.cast.gz", "claude/b.cast.gz"])`, the stub returns an empty vec].
- [ ] RED commit:
  - `all_selects_every_live_remote_session` [`assert_eq!(hits.len(), 3)`, tombstoned sessions excluded]
  - `pattern_without_match_is_an_error` [`assert_eq!(err, SelectError::NoMatch("x*".into()))`, one per pattern]
- [ ] GREEN commit: `select.rs`, a small hand written matcher for `*` and `?` (no new dependency), under 120 lines with tests.
- [ ] RED commit: `parses_delete_patterns_all_and_force` in `cli_sync_parse_test.rs` [`assert!(parse(["delete", "--all", "x"]).is_err())`, `assert!(parse(["delete", "--all"]).is_ok())`, `--force` exists].
- [ ] RED commit: `preview_is_collected_before_any_prompt` in `cli_sync_test.rs` [`assert!(out.find("2 sessions").unwrap() < out.find("Delete").unwrap())`, `-v` lists every session, `--dry-run` gives `assert_eq!(appends, 0)`].
- [ ] RED commit: `one_prompt_up_to_ten_matches` [`assert!(!out.contains("Type"))`, a no gives `assert!(out.contains("No changes made."))` and zero appends].
- [ ] RED commit: `over_ten_matches_asks_to_type_the_count` [`assert!(out.contains("Type 11 to confirm"))`, a wrong number gives `assert_eq!(appends, 0)`].
- [ ] RED commit: `yes_skips_only_the_first_prompt` [`assert!(out.contains("Type 11 to confirm"))` with `--yes`].
- [ ] RED commit: `over_ten_without_tty_needs_force` [`assert!(!st.success())` and `assert!(err.contains("--force"))` and zero appends, `--yes --force` gives 11 tombstones, one match without a TTY still needs `--yes` as today].
- [ ] RED commit: `no_remote_match_says_so_and_exits_non_zero` [`assert!(!st.success())` and `assert!(err.contains("no remote session matches"))`, local files untouched].
- [ ] RED commit: `pattern_delete_is_one_tombstone_append` [`assert_eq!(appends, 1)` and `assert_eq!(deleted_entries, 11)` at gen + 1 in the open pack, the local files stay, the dead bytes show up in `merge` so prune (Stage 61) frees them].
- [ ] RED commit: `delete_prints_compact_summary_and_full_list_with_v` [`assert!(lines(default) < 10)`] and `delete_loading_and_live_line_only_on_a_tty` [`assert!(!plain_out.contains(LOADING))`] (P12, as in Stage 56).
- [ ] GREEN commit per RED commit above. Each stub in the RED commits deletes without asking or returns `Ok(())` early, never `todo!()`.

Files: `src/sync/mod.rs`, `src/sync/engine/mod.rs`, `src/sync/engine/select.rs`, `src/sync/engine/run.rs`, `src/cli/subcommands.rs`, `src/commands/sync/mod.rs`, `src/commands/sync/run.rs`, `src/commands/sync/delete.rs`, `src/sync/app/format/summary.rs`, `tests/integration/cli_sync_parse_test.rs`, `tests/integration/cli_sync_test.rs`
Depends on: Stage 56

Considerations: `commands/sync/run.rs` is 350 lines, as such the delete command moves into its own `commands/sync/delete.rs` (prompts and the typed count, under 200 lines). The threshold 10 is a constant, not config. The prompt reads go through the existing `should_proceed` plus one new `typed_count` helper so tests can feed stdin.

### Stage 58: agr sync migrate

Goal: ADR "Migration from per-session gists", the local index v1 becomes v2 with its gens and the ~60 per-session gists and the `agr:pending` stubs of the user move into packs, before the first pack push.
Owner: implementer
PR: C

- [ ] RED commit: `stub_younger_than_one_hour_is_kept` in the pure `recognise` table test [`assert_eq!(kind, Kept)`, the stub calls every `agr:pending` a `Stub`].
- [ ] RED commit: the rest of the table [`assert_eq!(kind, Expected)` per row]: ours v1, stub over 1 hour, other key, unknown version, extra files.
- [ ] RED commit, local index:
  - `index_v1_gens_carry_into_v2` [`assert_eq!((e.sha256.as_str(), e.gen), ("h", 3))` per entry after the conversion, the stub starts every gen at 1]
  - `unsynced_local_edit_survives_migrate` [`assert_eq!(read(local), edited)` after migrate, and the next push plans it as `Push` at gen 4, the migrated gen 3 content never overwrites it]
- [ ] RED commit: `unreadable_v1_is_kept_and_named` [`assert!(report.lines.iter().any(|l| l.contains("gh gist delete") && l.contains("cannot be read")))` and `assert_eq!(deletes, 0)` for a v1 gist of our key whose part is missing, the stub keeps it silently].
- [ ] RED commit: `dry_run_counts_and_writes_nothing` [`assert_eq!(line, "2 sessions to move (3 KiB), 1 already moved, 4 empty stubs to delete, 1 not ours (kept)")` and `assert_eq!((appends, deletes), (0, 0))`, the local index stays v1].
- [ ] RED commit:
  - `moved_session_keeps_gen_and_sha` [`assert_eq!((e.gen, e.sha256), (3, sha))`]
  - `already_moved_is_skipped` [`assert_eq!(appends, 0)`]
  - `old_gist_deleted_only_after_the_index_shows_it` [`assert_eq!(deletes, 0)` when the re-read fails]
  - `sha_mismatch_is_kept_and_reported` [`assert!(report.kept.contains(id))` and `assert_eq!(deletes, 0)`]
  - `rate_limit_stops_with_n_left_and_rerun_finishes` [`assert!(out.contains("3 left, run again later"))`, then `assert_eq!(left, 0)` after the re-run]
  - `listing_past_3000_does_not_abort` [`assert!(report.is_ok())`]
  - `migrate_aborts_on_partial_index_view` [`assert_eq!((appends, deletes), (0, 0))`]
  - `migrate_opens_without_the_v1_guard_and_unblocks` [`assert!(Repo::open(..).is_ok())` once migrate deleted the last `agr:v1` gist of our key, and a gist kept for a hash mismatch is named with `gh gist delete <id>` in the report]
- [ ] GREEN commit per RED commit above.
- [ ] RED commit: `parses_migrate_yes` in `cli_sync_parse_test.rs` [`assert!(parse(["migrate", "--yes"]).is_ok())`], GREEN adds `agr sync migrate [--yes]`.
- [ ] RED commit: `migrate_prints_compact_summary_and_full_list_with_v` [`assert!(lines(default) < 10)`] and `migrate_loading_and_live_line_only_on_a_tty` [`assert!(!plain_out.contains(LOADING))`] (P12, as in Stage 56).

Files: `src/sync/mod.rs`, `src/sync/pack/mod.rs`, `src/sync/pack/migrate/mod.rs`, `src/sync/pack/migrate/recognise.rs`, `src/sync/pack/migrate/local.rs`, `src/sync/pack/migrate/tests.rs`, `src/sync/engine/index.rs`, `src/sync/backend/gist/mod.rs`, `src/cli/subcommands.rs`, `src/commands/sync/mod.rs`, `src/commands/sync/migrate.rs`, `src/dispatch.rs`, `tests/integration/cli_sync_parse_test.rs`, `tests/integration/cli_sync_test.rs`
Depends on: Stage 57

Considerations: migrate reads old gists through `&dyn SyncBackend` (not a concrete store, so the `pack/` rules hold) and tests run on `MemoryBackend`. `recognise.rs` pure under 150 lines, the v1 to v2 conversion in `local.rs` under 100, the move loop in `mod.rs` under 300. Migrate is run by hand, never automatically (ADR "Migration"), because it deletes remote gists and the user wants a dry-run first.

### Stage 59: Drop the per-session code

Goal: ADR "Kept and dropped" applied. `SyncBackend` shrinks to the read side migrate needs (`list`, `pull`, `delete`) under `store/legacy/`, the rest of `backend/` is gone.
Owner: implementer
PR: C

- [ ] RED commit: `violations_flags_any_backend_path` [`assert_eq!(violations("engine/x.rs", "use crate::sync::backend::X;").len(), 1)`, gives 0].
- [ ] GREEN commit: `backend/` rows replaced by `crate::sync::backend` for every layer, `runner.rs`, `pace.rs`, `progress.rs`, `scratch.rs`, `fake.rs` moved with `git mv` into `store/gist/`, `SPAWNERS` points at `store/gist/runner.rs`.
- [ ] RED commit (layer rows after the switch, ADR "Layers"):
  - `violations_flags_engine_naming_crypto_or_store` [`assert_eq!(violations("engine/x.rs", "use crate::sync::crypto::Key;").len(), 1)` and the same for `crate::sync::store::PackStore`, gives 0]
  - `violations_flags_legacy_outside_pack_migrate` [`assert_eq!(violations("app/x.rs", "use crate::sync::store::legacy::X;").len(), 1)` and `assert!(violations("pack/migrate/x.rs", ..).is_empty())`, gives 0]
- [ ] GREEN commit: `RULES` rows `("engine/", "crate::sync::crypto")` and `("engine/", "crate::sync::store")`, one `ONLY_IN` list next to `SPAWNERS` that allows `crate::sync::store::legacy` only under `pack/migrate/` and `store/`. The engine gets `sha256_hex` and the `Repo` types through `pack` re-exports, the app gets the old reader as `Box<dyn LegacyRead>` from `store::gist::legacy_reader()`.
- [ ] Deleted: `.keep`, `manifest`, `agr:pending` creation, description token, heal and repair, adopt own commit, pending resume, index id fallback, duplicate by `created_at`, `RemoteEntry`, `BackendError::Changed`, index fields `remote_id`, `token`, `pending`, the test-only `backend/file.rs`.
- [ ] `tests/sync_gist_live.rs` deleted, `tests/pack_gist_live.rs` covers the store. `tests/pack_probe_live.rs` stays as an ignored test (P11), it documents the verified facts.
- [ ] RED commit: `violations_flags_gist_literal_outside_gist_store` (P17) [`assert_eq!(violations("engine/x.rs", "let n = \"gist\";").len(), 1)`, gives 0]. GREEN adds the check to `layer_rules_hold`: a string literal containing `gist` (case insensitive, test code after `#[cfg(test)]` skipped) is allowed only under `store/gist/`, `store/legacy/` and in the one match arm of `app/setup.rs` that maps the config value `gist` to the gist store. The "gh not found" and `gh auth refresh -s gist` texts move into `store/gist/` as `StoreError` messages so the check holds.
- [ ] No behaviour change: the full test run is green before and after, `rg 'RemoteEntry|agr:pending|remote_id' src` only hits `store/legacy/` and `pack/migrate/local.rs`.

Files: `src/sync/mod.rs`, `src/sync/backend/mod.rs`, `src/sync/backend/memory.rs`, `src/sync/backend/file.rs`, `src/sync/backend/gist/mod.rs`, `src/sync/backend/gist/parse.rs`, `src/sync/backend/gist/parts.rs`, `src/sync/backend/gist/pace.rs`, `src/sync/backend/gist/progress.rs`, `src/sync/backend/gist/runner.rs`, `src/sync/backend/gist/scratch.rs`, `src/sync/backend/gist/fake.rs`, `src/sync/backend/gist/pull_tests.rs`, `src/sync/backend/gist/push_tests.rs`, `src/sync/store/mod.rs`, `src/sync/store/gist/mod.rs`, `src/sync/store/gist/runner.rs`, `src/sync/store/gist/pace.rs`, `src/sync/store/gist/progress.rs`, `src/sync/store/gist/scratch.rs`, `src/sync/store/gist/fake.rs`, `src/sync/store/legacy/mod.rs`, `src/sync/store/legacy/gist.rs`, `src/sync/store/legacy/memory.rs`, `src/sync/pack/mod.rs`, `src/sync/pack/migrate/mod.rs`, `src/sync/engine/index.rs`, `src/sync/engine/scan.rs`, `src/sync/engine/push.rs`, `src/sync/engine/pull.rs`, `src/sync/app/setup.rs`, `tests/sync_gist_live.rs`
Depends on: Stage 58

Considerations: removing the legacy reader for good is one release later (ADR), not in this plan. `store/legacy/gist.rs` keeps only the old pull path, under 250 lines.

### Stage 60: Docs, PR C gates, live migrate

Goal: docs match packs, all gates green, the user's real account is migrated.
Owner: implementer, live part run by the user
PR: C

- [ ] `docs/architecture/sync.md` with the layer, pack format and plan diagrams from the ADR, `docs/wiki/Command-sync.md` (push, pull, status, delete, migrate first, fork and `--overwrite`, deleted on the remote and how to restore), `docs/wiki/Configuration.md` (`pack_size_mib`), `README.md`, `docs/COMMANDS.md`.
- [ ] P16 privacy section in `docs/architecture/sync.md`: what a third party with the URL can see (ADR "What a third party with the URL can see"), including the random pack uuid in the description and that the `index` size per revision roughly shows how many sessions each append added.
- [ ] Gates as in Stage 53 plus `cargo build --release && ./tests/e2e_test.sh`, `git diff --stat origin/feature/sync-gist...HEAD` under 100 files.
- [ ] Live by the user: `tests/pack_gist_live.rs` once more, then `agr sync migrate` on the real account (dry-run line shows about 60 sessions plus the stubs), then `agr sync migrate --yes`, then `agr sync status` shows a few packs, the gist page shows only `agr:v2` entries.
- [ ] ADR section status Accepted (built), Open risks 1 and 3 closed with evidence.

Files: `docs/architecture/sync.md`, `docs/wiki/Command-sync.md`, `docs/wiki/Configuration.md`, `docs/COMMANDS.md`, `README.md`, `.state/feature-sync-gist/ADR.md`, `.state/feature-sync-gist/PLAN.md`
Depends on: Stage 59

### Stage 61: Prune planner and execution

Goal: ADR "Changes, deletes and `agr sync prune`", pure planner plus execution on `MemoryStore`, including `--seal-orphans`.
Owner: implementer
PR: D

- [ ] RED commit: `fully_dead_sealed_pack_is_deleted` [`assert_eq!(plan.delete, vec![id])`, the stub returns an empty plan].
- [ ] RED commit:
  - `dead_share_20_percent_is_repacked_19_is_kept` [`assert_eq!(plan.repack, vec![a])` and `assert!(!plan.repack.contains(&b))`]
  - `open_packs_are_never_planned` [`assert!(plan.is_empty())` for own and other writers]
  - `tombstones_are_copied` [`assert_eq!(plan.copy_tombstones, 1)`]
  - `live_tombstone_pack_is_not_fully_dead` [`assert!(plan.delete.is_empty())` and `assert_eq!(plan.repack, vec![id])` for a sealed pack whose only live entry is a top-gen tombstone, the stub counts tombstones as dead]
  - `two_places_tie_break_is_deterministic` [`assert_eq!(plan(view_ab).delete, plan(view_ba).delete)` and `assert_eq!(plan.delete, vec!["a"])`, lowest pack id loses whatever order the packs were listed in]
  - `prune_never_touches_unreadable_packs` [`assert!(!plan.touches(other_key) && !plan.touches(unknown_version))`]
  - `dry_run_reports_bytes_to_move_and_free` [`assert_eq!((plan.move_bytes, plan.free_bytes), (m, f))`]
- [ ] GREEN commit: `prune/plan.rs`.
- [ ] RED commit:
  - `copies_keep_path_gen_and_sha` [`assert_eq!((c.path, c.gen, c.sha256), (p, g, s))`]
  - `old_pack_deleted_only_after_copies_are_visible` [`assert_eq!(deletes, 0)` when the re-read fails]
  - `two_prunes_at_once_lose_nothing` [`assert_eq!(view.len(), before)`]
  - `concurrent_prunes_never_delete_the_last_place` [`assert!(view.get(p).unwrap().places.len() >= 1)` for every path after two prunes interleaved step by step on one `MemoryStore`, the stub deletes on the first view without the last place check]
  - `pull_during_prune_retries_with_the_copy` [`assert_eq!(sha, want)`]
  - `partial_index_view_aborts_push_delete_and_prune` [`assert_eq!((appends, deletes), (0, 0))` for engine push, engine delete and prune with `fail_read_index` on one pack, each returns `PartialView`]
- [ ] GREEN commit: `prune/mod.rs`.
- [ ] RED commit (`--seal-orphans`, ADR Open risk 4):
  - `seal_orphans_seals_idle_open_packs_of_other_writers` [`assert!(index.sealed)` and `assert_eq!(info.len, len_before)` for an open pack of another writer idle 31 days, a pack idle 29 days and the own open pack stay open, the stub seals nothing]
  - `sealed_orphan_owner_starts_a_fresh_pack` [`assert_eq!(store.list()?.packs.len(), 2)` after the original writer pushes again, its `Moved` retry sees a sealed pack]
  - `seal_orphans_loses_the_race_to_the_owner_and_drops_the_seal` [the owner appends between the orphan read and the seal, `assert_eq!(appends_by_prune, 1)` (no retry), `assert!(!index.sealed)`, the owner's entry is in the view and the report says "changed during the seal, skipped", the stub retries on the `Moved` text and seals over the owner]
  - `seal_orphans_dry_run_prints_download_size` [`assert!(out.contains("2 packs to seal, 96 MiB to download"))` and `assert_eq!(appends, 0)`, a real run stops at 1 GiB of download with "N left, run again"]
  - `seal_orphans_never_touches_a_duplicated_pack` [`assert!(!plan.seal.contains(&a))` for a pack with a copy]
- [ ] GREEN commit: `prune/orphans.rs`. The one append to another writer's pack: checks our key, not sealed and idle over 30 days instead of our writer id. Gist store side in `store/gist/append.rs`: an append to an id that is not the cached `open_pack` clones into a temp dir and deletes it when the append returns, covered by `orphan_seal_clone_is_not_kept` [`assert!(!cache_dir.join(id).exists())` after success and after an error].

Files: `src/sync/pack/mod.rs`, `src/sync/pack/prune/mod.rs`, `src/sync/pack/prune/plan.rs`, `src/sync/pack/prune/orphans.rs`, `src/sync/pack/prune/tests.rs`, `src/sync/store/gist/append.rs`, `src/sync/store/gist/tests.rs`, `tests/integration/sync_engine_test.rs`
Depends on: Stage 60

Considerations: `plan.rs` pure under 150 lines, execution under 250, `orphans.rs` under 100, dead bytes and the keeper come from `merge` (Stage 50).

### Stage 62: agr sync prune command, docs, PR D gates

Goal: `agr sync prune [--dry-run] [--seal-orphans]` under the local lock, documented, all gates green.
Owner: implementer
PR: D

- [ ] RED commit: `parses_prune_dry_run_and_seal_orphans` in `cli_sync_parse_test.rs` [`assert!(parse(["prune", "--dry-run", "--seal-orphans"]).is_ok())`] and `prune_while_locked_is_busy` in `cli_sync_test.rs` [`assert!(out.contains("busy"))`].
- [ ] RED commit: `prune_prints_compact_summary_and_full_list_with_v` [`assert!(lines(default) < 10)`] and `prune_loading_and_live_line_only_on_a_tty` [`assert!(!plain_out.contains(LOADING))`] (P12, as in Stage 56).
- [ ] GREEN commit: command, summary output.
- [ ] `docs/wiki/Command-sync.md` and `docs/architecture/sync.md` get prune and `--seal-orphans`, gates as in Stage 60, `git diff --stat origin/feature/sync-gist...HEAD` under 100 files.

Files: `src/sync/mod.rs`, `src/cli/subcommands.rs`, `src/commands/sync/mod.rs`, `src/commands/sync/prune.rs`, `src/dispatch.rs`, `src/sync/app/format/summary.rs`, `tests/integration/cli_sync_parse_test.rs`, `tests/integration/cli_sync_test.rs`, `docs/wiki/Command-sync.md`, `docs/architecture/sync.md`, `.state/feature-sync-gist/ADR.md`, `.state/feature-sync-gist/PLAN.md`
Depends on: Stage 61

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
29. Stage 42
30. Stage 43
31. Stage 44
32. Stage 45
33. Stage 46, Stage 47 (parallel, probe file vs new store module)
34. Stage 48, Stage 52 (parallel, pack index vs gist store read side)
35. Stage 49
36. Stage 50
37. Stage 51
38. Stage 53
39. Stage 54
40. Stage 55
41. Stage 56
42. Stage 57
43. Stage 58
44. Stage 59
45. Stage 60
46. Stage 61
47. Stage 62

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
| 40 | done | layers, facade, testing module, layer check |
| 41 | done | docs match the tree, 100 files changed vs main, largest file 370 lines, Command::new only in runner.rs and app/auto/mod.rs |
| 42 | todo | own STREAM module, ADR section of 2026-11-07 |
| 43 | todo | |
| 44 | todo | chacha20poly1305 0.11 |
| 45 | todo | |
| 46 | todo | PR B, probe, run by the user |
| 47 | todo | PR B |
| 48 | todo | PR B |
| 49 | todo | PR B |
| 50 | todo | PR B |
| 51 | todo | PR B |
| 52 | todo | PR B |
| 53 | todo | PR B |
| 54 | todo | PR C |
| 55 | todo | PR C |
| 56 | todo | PR C |
| 57 | todo | PR C, pattern delete (P10) |
| 58 | todo | PR C |
| 59 | todo | PR C |
| 60 | todo | PR C |
| 61 | todo | PR D |
| 62 | todo | PR D |
