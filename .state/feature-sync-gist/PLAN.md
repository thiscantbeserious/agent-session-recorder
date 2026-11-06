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

Files: `src/sync/mod.rs`, `src/sync/scan.rs`, `src/sync/run.rs`,
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

- [ ] RED/GREEN: `auto::decide(config, state, key_present, path)` returns `Skip(hint)` for no
      backend (silent), `auto_push = false` (silent), no privacy ack (hint to run
      `agr sync push` once), no key, plain `.cast`, live lock, `gh`/`git` not on PATH, else
      `Spawn(args)` with `sync push --auto <agent>/<name>`.
- [ ] RED/GREEN: the `--auto` run never prompts, never pulls or deletes, skips conflicts, writes
      one-line `last_auto_push_error` to state on failure, returns quietly on `Busy`.
- [ ] GREEN: hook in `Recorder::finish` after `finalize_recording`, prints at most one hint
      line, spawn with null stdio and `process_group(0)` on unix, no wait.
- [ ] `src/recording.rs` stays under ~400 lines (logic lives in `src/sync/auto.rs`).

Files: `src/sync/mod.rs`, `src/sync/auto.rs`, `src/recording.rs`,
       `tests/integration/recording_test.rs`
Depends on: Stage 8

### Stage 10: Docs

Goal: README, config docs and generated docs describe sync, encryption and auto push.
Owner: implementer

- [ ] README section "Syncing Sessions": `[sync]`, the commands, key export and import, one
      gist per session, 100MB cap, `gh` and `git` needed, auto push default.
- [ ] Privacy statement in README and config docs: secret gists are readable with the link,
      uploads are encrypted locally, gist descriptions hold only an opaque token, losing every
      copy of the key makes synced sessions unrecoverable.
- [ ] `cargo xtask gen-docs`, commit the regenerated files.

Files: `README.md`, `src/config/docs/templates.rs`, `docs/COMMANDS.md`, `docs/man/`, `docs/wiki/`
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

## Batches

1. Stage 1, Stage 2, Stage 3 (parallel, no shared files)
2. Stage 4
3. Stage 5
4. Stage 6
5. Stage 7
6. Stage 8
7. Stage 9, Stage 10 (parallel)
8. Stage 11

Stages 4 to 7 share `src/sync/mod.rs`, as such they run in order.

## Progress

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | done | also `zeroize = "1"` (already pulled in by the cipher) |
| 2 | pending | |
| 3 | pending | |
| 4 | done | |
| 5 | done | |
| 6 | done | |
| 7 | done | live test written but not run, see ADR "To verify" |
| 8 | done | test-only `AGR_SYNC_TEST_BACKEND_DIR` file backend (`src/sync/file.rs`) |
| 9 | pending | |
| 10 | pending | |
| 11 | pending | |
