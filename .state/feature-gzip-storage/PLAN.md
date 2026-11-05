# Plan: Gzip-Compressed Session Storage

References: ADR.md, REQUIREMENTS.md

Every stage is TDD (RED test first, GREEN minimal code, REFACTOR), ends with `cargo fmt`,
`cargo clippy -- -D warnings` and `cargo test`. No trivial closures where a method reference
works (SonarCloud rust:S1612). New files stay under ~400 lines, functions ~20 lines, nesting
max 3. New tests go in `tests/integration/`.

## Open Questions

1. Exact projected-size sample length (ADR says 64KiB), tune in Stage 9 if the estimate is
   more than ~20% off on the fixture set.
2. Whether `SessionPreview` or `FileItem` carries the uncompressed size for the detail pane
   (Stage 13, see ADR "To verify").

## Stages

### Stage 1: Codec module and flate2 dependency

Goal: `src/files/codec.rs` with suffix helpers, `open_reader`, `atomic_write`, `transcode`,
`same_content`, `uncompressed_size`. No callers yet.
Owner: implementer

- [x] Add `flate2 = { version = "1.1", default-features = false, features = ["rust_backend"] }`.
- [x] RED/GREEN: `is_gzip_path`, `is_session_path` (accepts `x.cast`, `x.cast.gz`, rejects
      `.x.cast.gz.bak`, `.x.cast.gz.tmp`, `.x.cast.lock`, `x.gz`, `x.txt`).
- [x] RED/GREEN: `session_stem` strips `.cast.gz` then `.cast`, `session_suffix` returns the
      matching suffix.
- [x] RED/GREEN: `temp_path_for(dir/x.cast.gz)` is `dir/.x.cast.gz.tmp` (via
      `hidden_auxiliary_path`).
- [x] RED/GREEN: `atomic_write` to `.cast.gz` yields a file `GzDecoder` decodes to the input,
      to `.cast` yields plain bytes, temp removed on write error.
- [x] RED/GREEN: determinism, two writes of the same content are byte-identical, header has
      mtime 0, no FNAME.
- [x] RED/GREEN: `open_reader` on plain and gzip yields the same lines.
- [x] RED/GREEN: truncated `.cast.gz` and flipped trailer byte give an error whose message
      contains the file path.
- [x] RED/GREEN: `transcode` plain to gzip and back is lossless, `same_content` true for a
      plain/gzip pair and false after one byte changes.
- [x] RED/GREEN: `uncompressed_size` returns trailer ISIZE for gzip, file length for plain,
      `Over4GiB` when the 64x wrap rule triggers (unit test on the pure decision function with
      synthetic numbers, no 4GiB file).

Files: `Cargo.toml`, `Cargo.lock`, `src/files/codec.rs`, `src/files/mod.rs`,
       `tests/integration/codec_test.rs`, `tests/integration.rs`
Depends on: none

Considerations:
- `atomic_write` takes `FnOnce(&mut dyn Write) -> Result<()>` so `AsciicastFile::write_to` and
  `io::copy` both fit. Call `GzEncoder::finish()` before `sync_all`.
- Gzip output must be single-member and readable by `gzip -dc` (e2e check in Stage 16).

### Stage 2: Config v4 and Compression enum

Goal: `[storage] compression = "gzip" | "none"`, default gzip, versioned migration v4.
Owner: implementer

- [x] RED: `migrate_config("config_version = 3\n[storage]\n")` yields `compression = "gzip"` and
      `config_version = 4`, field listed in `added_fields`.
- [x] RED: an existing `compression = "none"` is kept by v4.
- [x] RED: loading `compression = "zstd"` fails with a message naming the allowed values.
- [x] GREEN: `Compression` enum (`serde(rename_all = "lowercase")`, `Default = Gzip`) on
      `StorageConfig`, `migrations/v4.rs`, register in `MIGRATIONS`, `CURRENT_VERSION = 4`.
- [x] Add the key to `src/config/docs.rs` field docs, accept the updated default-config
      snapshot (only the new line may change).

Files: `src/config/types.rs`, `src/config/migrate/mod.rs`,
       `src/config/migrate/migrations/mod.rs`, `src/config/migrate/migrations/v4.rs`,
       `src/config/docs.rs`, `src/config/migrate/snapshots/agr__config__migrate__tests__complete_default_config_snapshot.snap`,
       `tests/integration/config_test.rs`
Depends on: none

Considerations:
- Rebase onto main after #162 merges, v3 must exist.
- `src/config/migrate/mod.rs` is over 400 lines already, only the constant changes here.

### Stage 3: Split src/storage/mod.rs (no behavior change)

Goal: bring `src/storage/mod.rs` (1012 lines) under the limit before adding logic.
Owner: implementer

- [x] Move `SessionInfo`, `format_smart_time`, `list_sessions`, `resolve_cast_path`,
      `find_cast_file_by_name`, `list_cast_files_short` to `src/storage/sessions.rs`
      (as `impl StorageManager` blocks where they are methods).
- [x] Move `StorageStats`, `get_stats`, `calculate_disk_percentage`, `query_disk_total_bytes`
      to `src/storage/stats.rs`.
- [x] Move `ImportError`, `validate_agent_name`, `validate_cast_header`,
      `resolve_filename_conflict`, `import_cast_file` to `src/storage/import.rs`.
- [x] Re-export from `mod.rs` so every existing `crate::storage::X` / `agr::storage::X` import
      still resolves.
- [x] Move the inline `#[cfg(test)]` tests to `tests/integration/storage_test.rs` (private
      helpers they need become `pub(crate)` or are tested through the public API).
- [x] `cargo test` passes with zero changes outside `src/storage/` and the test file.

Files: `src/storage/mod.rs`, `src/storage/sessions.rs`, `src/storage/stats.rs`,
       `src/storage/import.rs`, `tests/integration/storage_test.rs`
Depends on: none

Considerations:
- Pure move, review with `git diff --color-moved`.

### Stage 4: Offer state file

Goal: `~/.config/agr/state.toml` with `compress_offer_version`.
Owner: implementer

- [x] RED: load from a missing file returns default (`None`).
- [x] RED: load from a malformed file returns default, does not error.
- [x] RED: save then load round-trips `"0.3.0"`, save is atomic (temp + rename).
- [x] GREEN: `src/config/state.rs` with `State { compress_offer_version: Option<String> }`,
      `load_from(path)`, `save_to(path)`, plus `load()` / `save()` using `config_dir()`.

Files: `src/config/state.rs`, `src/config/mod.rs`, `tests/integration/config_test.rs`
Depends on: Stage 2, Stage 18

Considerations:
- Path-taking variants keep tests off the real home directory.

### Stage 5: Asciicast read, write and repair through the codec

Goal: parse, write, diagnose and repair work on `.cast.gz`, and writes keep the format.
Owner: implementer

- [x] RED: `AsciicastFile::parse(x.cast.gz)` equals parse of the plain twin.
- [x] RED: `write(x.cast.gz)` produces gzip, `write(x.cast)` plain, no `x.cast.cast.tmp` left.
- [x] RED: `diagnose` and `repair` on a gzip file with a corrupt line, the repaired file is
      still gzip and the bad line is gone.
- [x] RED: `MarkerManager::add_marker` on `.cast.gz` keeps the file gzip.
- [x] GREEN: `parse` uses `codec::open_reader`, `write` uses `codec::atomic_write`, `diagnose`
      uses `open_reader`, `repair` streams `open_reader` into `atomic_write` (drops
      `read_to_string`).

Files: `src/asciicast/reader.rs`, `src/asciicast/writer.rs`, `src/asciicast/integrity.rs`,
       `tests/integration/asciicast_test.rs`, `tests/integration/markers_test.rs`
Depends on: Stage 1, Stage 18

Considerations:
- Inline tests in `integrity.rs` and `transform_ops.rs` assert `with_extension("cast.tmp")`
  is absent, update those to `codec::temp_path_for`.

### Stage 6: Backup, restore and rename keep the full suffix

Goal: `.x.cast.gz.bak` byte copies, restore via hidden temp, rename keeps `.cast.gz`.
Owner: implementer

- [ ] RED: `create_backup(x.cast.gz)` creates `.x.cast.gz.bak` byte-identical to the source.
- [ ] RED: `restore_from_backup(x.cast.gz)` restores bytes exactly, no `x.cast.cast.tmp`.
- [ ] RED: `rename_file(dir/x.cast.gz, "y")` gives `dir/y.cast.gz` and moves the backup to
      `.y.cast.gz.bak`.
- [ ] RED: rename length check counts the full `.cast.gz` suffix.
- [ ] GREEN: `restore_from_backup` temp via `codec::temp_path_for`, `rename_file` uses
      `codec::session_stem`/`session_suffix` instead of `extension()`.
- [ ] REFACTOR: `transform_ops::apply_transforms` calls `backup::create_backup` instead of its
      own `fs::copy` (one backup code path).

Files: `src/files/backup.rs`, `src/files/filename.rs`, `src/asciicast/transform_ops.rs`,
       `tests/integration/backup_gzip_test.rs`, `tests/integration.rs`
(New test file instead of `filename_test.rs`, which is 2807 lines.)
Depends on: Stage 1, Stage 5, Stage 20, Stage 23

Considerations:
- `transform_ops` writes through `AsciicastFile::write`, as such its gzip test needs Stage 5.

### Stage 7: Session listing, resolve and import handle both formats

Goal: sessions listed once by stem, lookup accepts either name, import compresses per config.
Owner: implementer

- [x] RED: `list_sessions` returns `x.cast` and `y.cast.gz`, never hidden `.y.cast.gz.bak`
      or `.y.cast.gz.tmp`.
- [x] RED: with both `x.cast` and `x.cast.gz` present only `x.cast.gz` is listed.
- [x] RED: `resolve_cast_path("agent/x.cast")` finds `x.cast.gz` when only that exists, and
      the other way round, same for `find_cast_file_by_name` and absolute paths.
- [x] RED: completions (`list_cast_files_short`) include `.cast.gz` sessions.
- [x] RED: import of `.cast.gz` with valid content succeeds, with non-asciicast content fails
      `InvalidFormat`, a plain `.cast` under gzip config lands as `<name>.cast.gz`, a
      `.cast.gz` under `none` lands as plain `<name>.cast`.
- [x] RED: conflict suffixing works on `.cast.gz` (`x-1.cast.gz`) and checks both variants.
- [x] GREEN: `is_session_path` in listing, dedupe by stem, resolve fallbacks, header check via
      `open_reader`, import via `codec::transcode` (or `fs::copy` when formats match).

Files: `src/storage/sessions.rs`, `src/storage/import.rs`,
       `tests/integration/storage_list_test.rs`, `tests/integration/storage_lookup_test.rs`,
       `tests/integration/storage_import_test.rs`, `tests/integration/import_test.rs`
Depends on: Stage 1, Stage 2, Stage 3, Stage 17, Stage 18

Considerations:
- `ImportError::WrongExtension` message becomes "Expected .cast or .cast.gz".
- TUI import (`list_app.rs`) calls `import_cast_file`, no change needed there.

### Stage 8: Storage tracking with uncompressed size

Goal: on-disk size stays primary, uncompressed size shown in status and list.
Owner: implementer

- [ ] RED: `SessionInfo` exposes `uncompressed_size()` via `codec::uncompressed_size` (AG
      header subfield since Stage 18, trailer fallback for foreign files). Test proves there
      is no decompress: a file with a valid header and garbage body still returns the size.
- [ ] RED: `StorageStats.total_size` equals the sum of on-disk sizes, a new
      `uncompressed_total` is the sum of uncompressed sizes (or flags "4GiB+" when any part
      overflowed).
- [ ] RED: `summary()` shows both numbers, plus plain session count.
- [ ] RED: `exceeds_threshold` compares on-disk size.
- [ ] GREEN: implement in `stats.rs`, `agr list` table gets an uncompressed column.

Files: `src/storage/stats.rs`, `src/commands/status.rs`, `src/commands/list.rs`,
       `src/commands/cleanup.rs`, `tests/integration/storage_stats_test.rs`
Depends on: Stage 7, Stage 20, Stage 23

Considerations:
- `commands/cleanup.rs` builds a mock `StorageStats`, add the new field there in the same
  stage if it breaks compilation (it is only a test helper).

### Stage 9: Compress core (shared by command, offer and recording)

Goal: `src/storage/compress.rs` with `plan`, `estimate`, `compress_file`, `run`, report.
Owner: implementer

- [ ] RED: `compress_file(x.cast)` leaves only `x.cast.gz`, content equal, no temp left.
- [ ] RED: failure injection (read-only dir, verify mismatch) leaves `x.cast` untouched and
      no `x.cast.gz` or temp behind.
- [ ] RED: an existing `.x.cast.bak` becomes `.x.cast.gz.bak` with equal decoded content.
- [ ] RED: `plan` skips sessions with a live lock (current PID) and reports them, a stale lock
      is cleaned and the file is eligible.
- [ ] RED: `plan` with an agent filter only includes that agent.
- [ ] RED: `run` continues after a per-file failure, report counts compressed, skipped,
      failed, bytes saved.
- [ ] RED: idempotent, a second `run` finds nothing to do.
- [ ] RED: leftover `x.cast` next to an equal `x.cast.gz` is removed, a differing one is
      skipped with a warning.
- [ ] RED: `estimate` is within 2x of the real size on the fixture files.
- [ ] GREEN: implement, `pub mod compress;` in `storage/mod.rs`.

Files: `src/storage/compress.rs`, `src/storage/mod.rs`, `tests/integration/compress_test.rs`,
       `tests/integration.rs`
Depends on: Stage 1, Stage 3, Stage 6, Stage 7, Stage 20, Stage 23

Considerations:
- No printing in this module, it returns data, commands do the output.

### Stage 10: Compress after recording

Goal: finished recordings become `.cast.gz` when `compression = "gzip"`.
Owner: implementer

- [ ] RED: helper `finalize_recording(path, &config)` returns `x.cast.gz` with gzip config and
      `x.cast` with `none`.
- [ ] RED: on compress failure it returns the plain path and the plain file still exists.
- [ ] GREEN: call it in `Recorder::record` after `remove_lock` and `resolve_actual_path`, use
      its result for the final box line and `maybe_auto_analyze`, print a warning on failure.

Files: `src/recording.rs`, `tests/integration/recording_test.rs`
Depends on: Stage 2, Stage 9

Considerations:
- Live recording code (lock, inode, header recovery) stays untouched and plain.
- Keep `recording.rs` under 400 lines, the helper is a few lines delegating to
  `compress::compress_file`.

### Stage 11: agr compress command

Goal: `agr compress [--agent X] [--yes]` with dry-run preview and confirm prompt.
Owner: implementer

- [ ] Move `should_proceed` from `commands/config.rs` to `commands/prompt.rs`, config migrate
      and reset use it (no behavior change, existing tests pass).
- [ ] RED: CLI parse tests for `compress`, `--agent`, `--yes` (in `tests/integration`).
- [ ] RED: help snapshot for `agr compress --help`, main help snapshot lists it.
- [ ] RED: `agr compress --yes` (assert_cmd, temp storage via config env or HOME) compresses
      and prints the report, without `--yes` and no TTY it prints the preview and changes
      nothing.
- [ ] GREEN: `Commands::Compress` in `cli.rs`, dispatch in `main.rs`, handler in
      `commands/compress.rs` (preview print, prompt, `compress::run`, report print).

Files: `src/cli.rs`, `src/dispatch.rs`, `src/commands/mod.rs`, `src/commands/compress.rs`,
       `src/commands/prompt.rs`, `src/commands/config.rs`,
       `tests/integration/snapshot_cli_test.rs`, `tests/integration/snapshots/`,
       `tests/integration/compress_test.rs`
Depends on: Stage 9, Stage 21, Stage 23

Considerations:
- Dispatch lives in `src/dispatch.rs` since Stage 21, add the arm there. Every function this
  stage changes ends within ~20 lines (`dispatch` is a dispatch-only match, allowed).

### Stage 12: Auto-migrate offer and status savings line

Goal: once-per-version offer on list/status/cleanup, `agr status` shows plain count and
projected savings.
Owner: implementer

- [ ] RED: gate function `should_offer(is_tty, state_version, current_version, plain_count)`
      truth table (no TTY never, same version never, zero plain never).
- [ ] RED: accepting and declining both save the current version, a second call does not offer.
- [ ] RED: non-TTY run of `agr status` never prompts and never rewrites (assert_cmd).
- [ ] RED: `agr status` with plain sessions shows "N plain, ~X saved by agr compress".
- [ ] GREEN: `commands/compress_offer.rs` (gate, reuses preview and run from
      `commands/compress.rs`), called at the start of `list`, `status`, `cleanup` handlers.

Files: `src/commands/compress_offer.rs`, `src/commands/mod.rs`, `src/commands/list.rs`,
       `src/commands/status.rs`, `src/commands/cleanup.rs`, `tests/integration/compress_test.rs`
Depends on: Stage 4, Stage 8, Stage 11

Considerations:
- Version is `env!("CARGO_PKG_VERSION")`.
- TTY check uses `atty` (already a dependency) on stdin and stdout.

### Stage 13: TUI on compressed sessions

Goal: ls/cleanup TUI preview, rename, analyze follow-up and detail pane work on `.cast.gz`.
Owner: implementer

- [ ] RED: `SessionPreview::load` on `.cast.gz` equals the plain twin (duration, markers,
      preview lines).
- [ ] RED: rename spans show the real suffix (`.cast.gz`) instead of a hardcoded `.cast`.
- [ ] RED: rename stem length limit uses the full suffix.
- [ ] RED: detail pane shows on-disk size and uncompressed size (snapshot).
- [ ] RED: cleanup TUI deletes a `.cast.gz` with its hidden backup and reports on-disk size
      freed.
- [ ] GREEN: `load_streaming` via `codec::open_reader`, suffix via `codec::session_suffix`,
      newest-session lookup after analyze via `codec::is_session_path`, rename stem via
      `codec::session_stem`, cleanup glob hint mentions `*.cast*`.

Files: `src/tui/widgets/file_explorer/`, `src/tui/list_app/`, `src/tui/cleanup_app/`,
       `tests/integration/preview_test.rs`, `tests/integration/file_explorer_test.rs`,
       `tests/integration/snapshot_tui_test.rs`
Depends on: Stage 1, Stage 7, Stage 8, Stage 21, Stage 23

Considerations:
- These three are module directories since Stage 21. Every file touched here stays within
  ~400 lines. Every function this stage changes ends within ~20 lines (user decision
  2026-11-05: only functions gzip touches, untouched moved functions stay as they are).

### Stage 14: CLI single-file commands

Goal: play, analyze, optimize, marker, copy accept `.cast.gz` without warnings, analyze
rename keeps the suffix.
Owner: implementer

- [ ] RED: `agr play`/`optimize`/`marker list` on `.cast.gz` give no extension warning
      (assert_cmd stderr).
- [ ] RED: analyze rename suggestion on `x.cast.gz` produces `<suggested>.cast.gz` and moves
      the backup (use `filename::rename_file`).
- [ ] RED: `agr optimize --remove-silence x.cast.gz` keeps gzip, `--output y.cast` writes
      plain.
- [ ] GREEN: replace `extension() != Some("cast")` with `!codec::is_session_path`, analyze
      rename through `rename_file`.

Files: `src/commands/play.rs`, `src/commands/analyze.rs`, `src/commands/transform.rs`,
       `src/commands/copy.rs`, `src/commands/marker.rs`, `tests/integration/play_test.rs`,
       `tests/integration/transform_test.rs`
Depends on: Stage 1, Stage 5, Stage 6, Stage 19, Stage 20, Stage 23

Considerations:
- `analyze.rs` is within the limit since Stage 19 (392 lines), keep it there.

### Stage 15: Clipboard always plain

Goal: `.cast.gz` copies as a plain `.cast` file reference or decompressed text.
Owner: implementer

- [x] RED: with a fake file-capable tool, copying `x.cast.gz` hands the tool a path ending in
      `x.cast` whose content equals the decompressed session.
- [x] RED: the temp dir is cleared before the next copy (at most one leftover) and removed
      when every tool fails.
- [x] RED: text fallback for `.cast.gz` copies decompressed text, size limit checked against
      the uncompressed size (a small gzip with a big ISIZE is rejected `FileTooLarge`).
- [x] RED: plain `.cast` path unchanged (existing tests pass untouched).
- [x] GREEN: `clipboard/plain_copy.rs` (`prepare(path) -> PlainSource`), `Copy::file` asks it
      for the path and size before trying tools.

Files: `src/clipboard/copy.rs`, `src/clipboard/plain_copy.rs`, `src/clipboard/mod.rs`,
       `tests/integration/clipboard_test.rs`
Depends on: Stage 1, Stage 18

Considerations:
- Pending user decision on temp lifetime (ADR section 7). If the user insists on immediate
  removal, the file-reference path is skipped for gzip and text fallback is used.

### Stage 16: Docs, e2e and memory check

Goal: README and config docs describe the feature, e2e covers zcat and recording, memory
check on a large file.
Owner: implementer

- [ ] README: `compression` key, `.cast.gz` naming, `agr compress`, offer behavior, state file.
- [ ] `src/config/docs.rs` example block includes `compression = "gzip"`.
- [ ] e2e: a finished recording is `.cast.gz` and `gzip -dc` output is valid asciicast (not `zcat`, which is the legacy .Z tool on macOS), with
      `none` it stays `.cast`.
- [ ] Performance test: generated 100MB-decompressed `.cast.gz`, parse peak RSS within ~10%
      of the plain file (ignored by default like the other performance tests).
- [ ] Full `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`,
      `./tests/e2e_test.sh`.

Files: `README.md`, `src/config/docs.rs`, `tests/e2e/recording.sh`,
       `tests/performance/gzip_memory_test.rs`, `tests/performance.rs`
Depends on: Stage 10, Stage 12, Stage 13, Stage 14, Stage 15

Considerations:
- Run SonarCloud locally if available, otherwise check the PR analysis for zero new issues.

### Stage 17: Batch 1 hardening (user request, in this MR)

Goal: no deferred cleanup. Prove Batch 1 tests catch regressions, bring the storage test
file under the size limit, make clippy clean for all targets.
Owner: implementer

- [x] Split `tests/integration/storage_test.rs` (~1300 lines) by topic so every file is
      within ~400 lines, no test removed or renamed, identical test count.
- [x] Fix every `cargo clippy --all-targets -- -D warnings` error (lib test modules,
      `src/commands/analyze.rs`, `src/config/migrate/mod.rs`,
      `src/analyzer/transforms/cleaner.rs`, integration tests incl. `codec_test.rs`).
- [x] Add `--all-targets` to the CI lint job so this stays clean.

Files: `tests/integration/storage_test.rs`, `tests/integration/storage_*_test.rs`,
       `tests/integration.rs`, `tests/integration/codec_test.rs`,
       `tests/integration/aggressive_transform_test.rs`,
       `tests/integration/snapshot_tui_test.rs`,
       `tests/integration/terminal_transform_test.rs`, `src/commands/analyze.rs`,
       `src/config/migrate/mod.rs`, `src/analyzer/transforms/cleaner.rs`,
       `.github/workflows/ci.yml`
Depends on: Stage 1, Stage 2, Stage 3

### Stage 18: Batch 1 pair review fixes (user request, in this MR)

Goal: fix every Batch 1 pair review finding. Exact uncompressed size per revised ADR.
Owner: implementer

- [x] RED/GREEN: `atomic_write` to `.cast.gz` stores the exact uncompressed size in a gzip
      FEXTRA subfield `AG` (len 8, u64 LE), placeholder then seek-and-patch before sync.
      `zcat`/`GzDecoder` still decode, two writes stay byte-identical.
- [x] RED/GREEN: `uncompressed_size` reads the subfield (1GB/100MB-sized synthetic header
      returns `Exact`), falls back to trailer for files without it, `Over4GiB` only when
      ISIZE < on-disk size. Replace `size_from_trailer` rule and its tests, regression test
      for 1GB at 10x returning `Exact`.
- [x] RED/GREEN: write-side errors (`File::create`, closure, `finish`, `sync_all`) carry the
      path via `with_context`.
- [x] Replace the `Option::take` wrapper in `atomic_write` with a direct `FnOnce` pass.
- [x] fsync the parent directory after rename (unix), error carries the dir path.
- [x] `// ponytail:` comment on the fixed temp path (concurrent writers to the same target).
- [x] Remove unused `Write` import and `read_all` helper in `codec_test.rs`.
- [x] RED/GREEN: invalid `compression` value error names the config file path (ADR).
- [x] Verify Stage 3 moved tests: compare old inline `create_test_config` fixture
      (threshold `Some(5.0)`, agents claude/codex, no_wrap) with the new one, restore the
      original fixture values if any moved test depended on them.
- [x] Mutation check for Stage 1, 2 and 18 tests (after the fixes above): for each codec/v4 behavior, break the code
      on purpose (e.g. set mtime, skip temp cleanup, drop path from error, wrong wrap rule,
      v4 overwriting an existing value), confirm a test fails, revert. Record the
      break/test pairs in this stage's notes. Add tests where nothing fails.

Files: `src/files/codec.rs`, `tests/integration/codec_test.rs`, `src/config/io.rs`,
       `tests/integration/config_test.rs`, `tests/integration/storage_test.rs`
Depends on: Stage 17

Notes:
- Stage 3 fixture verdict: old inline `create_test_config(storage_dir)` is today's
  `helpers::storage::create_dir_config` with identical values (threshold `Some(5.0)`, agents
  claude/codex, no_wrap empty), the other helper was always `Config::default()` + directory.
  Nothing to restore.
- `Config::load_from(path)` added so the invalid-value error is testable without HOME.
- `codec_size_test.rs` split from `codec_test.rs` to stay under 400 lines.
- Mutation table (break -> failing test):

| Break | Failing test |
|-------|--------------|
| mtime set non-zero | `codec_test::gzip_header_has_zero_mtime_and_no_name` |
| no temp cleanup on error | `codec_test::atomic_write_removes_temp_on_error` |
| path dropped from read error | `codec_test::truncated_gzip_error_names_path` |
| path dropped from open error | `codec_test::open_reader_missing_file_names_path` |
| path dropped from create error | `codec_test::atomic_write_create_error_names_temp_path` |
| path dropped from write error | `codec_test::atomic_write_closure_error_names_target_path` |
| FEXTRA size not patched | `codec_size_test::gzip_header_stores_exact_size_in_ag_subfield` |
| trailer fallback inverted | `codec_size_test::trailer_fallback_1gb_at_10x_is_exact` |
| v4 overwrites existing none | `migrate::migrations::v4::tests::v4_keeps_existing_none` |
| `is_session_path` accepts `.x.cast.gz.bak` | `codec_test::session_path_rejects_hidden_bak_of_gzip` (new, nothing failed before) |
| `is_session_path` ignores hidden prefix | `codec_test::session_path_rejects_hidden_even_with_session_suffix` (new) |
| `session_stem` not stripping `.gz` | `codec_test::session_stem_strips_known_suffixes` |
| `same_content` ignores length | `codec_test::same_content_detects_length_mismatch` (new) |

### Stage 19: Files touched by this branch within the size limit (user request, in this MR)

Goal: every file this branch touched is within ~400 lines. Pure moves, no behavior change.
Owner: implementer

- [x] Move inline `#[cfg(test)]` tests out of `src/config/migrate/mod.rs` (1055),
      `src/commands/analyze.rs` (699), `src/analyzer/transforms/cleaner.rs` (536) into
      `tests/integration/` where they only need the public API. Keep inline only tests that
      need private items, and split the source file if it is still over the limit.
- [x] Split `tests/integration/snapshot_tui_test.rs` (1232) by widget/screen into files within
      ~400 lines. Snapshot names must stay identical (pass the explicit name to
      `assert_snapshot!`), no `.snap` file renamed or changed.
- [x] Bring `tests/integration/aggressive_transform_test.rs` (409),
      `tests/integration/storage_import_test.rs` (404), `tests/integration/import_test.rs`
      (404) within the limit.
- [x] Test names and counts identical before and after (diff of sorted test names).

Files: `src/config/migrate/mod.rs`, `src/commands/analyze.rs`,
       `src/analyzer/transforms/cleaner.rs`, `tests/integration/snapshot_tui_test.rs`,
       `tests/integration/snapshot_tui_*_test.rs`, `tests/integration/aggressive_transform_test.rs`,
       `tests/integration/storage_import_test.rs`, `tests/integration/import_test.rs`,
       `tests/integration/config_migrate_test.rs`, `tests/integration/analyze_command_test.rs`,
       `tests/integration/cleaner_test.rs`, `tests/integration.rs`
Depends on: Stage 17, Stage 18

### Stage 20: Batch 4 pair review fixes (user request, in this MR)

Goal: fix every Batch 4 review finding.
Owner: implementer

- [x] RED/GREEN: `diagnose` on a truncated `.cast.gz` returns within a timeout (test runs it
      on a thread with `recv_timeout`), records the decode error once and stops. Confirmed
      hang today.
- [x] RED/GREEN: `repair` and `diagnose` agree on what a bad line is (invalid UTF-8 and JSON
      errors are bad lines in both, decoder errors are terminal in both).
- [x] RED/GREEN: clipboard file reference for a `.cast.gz` above 10MB decompressed succeeds
      (no up-front size reject), text fallback still enforces the limit on the decompressed
      size. One size check only, in the text path.
- [x] RED/GREEN: temp dir is `<tmp>/agr-clipboard-<uid>` (unix uid via `libc::getuid`),
      refused when it is a symlink or owned by another uid, created with mode 0700.
- [x] RED/GREEN: `resolve_target` returns an error after exhausting collision suffixes
      instead of overwriting the last candidate.
- [x] RED/GREEN: dir fsync failure after a successful rename does not report the write as
      failed (warn on stderr, return Ok).
- [x] RED/GREEN: `open_reader` decodes multi-member gzip (`MultiGzDecoder`), test with two
      concatenated members, same for `uncompressed_size` trailer fallback (documented limit).
- [x] `state.toml` broken file: warn on stderr once when it cannot be parsed.
- [x] Every function touched in Batch 4 within ~20 lines and nesting <= 3 (`repair`,
      `write_gzip`, `read_first_line` and others).

Files: `src/asciicast/integrity.rs`, `src/clipboard/plain_copy.rs`, `src/clipboard/copy.rs`,
       `src/storage/import.rs`, `src/files/codec.rs`, `src/config/state.rs`,
       `tests/integration/asciicast_test.rs`, `tests/integration/clipboard_gzip_test.rs`,
       `tests/integration/storage_import_gzip_test.rs`, `tests/integration/codec_test.rs`,
       `tests/integration/codec_size_test.rs`, `tests/integration/config_test.rs`
Depends on: Stage 4, Stage 5, Stage 7, Stage 15, Stage 19

### Stage 21: Split TUI files and main.rs before their gzip changes (user request, in this MR)

Goal: `src/tui/list_app.rs` (2134), `src/tui/widgets/file_explorer.rs` (2109),
`src/tui/cleanup_app.rs` (981), `src/main.rs` (1034) each split by responsibility into
modules within ~400 lines. Pure move, no behavior change.
Owner: implementer

- [x] Sorted test names identical before and after, snapshot files unchanged, no `.snap.new`.
- [x] Public paths used elsewhere keep working via re-exports.
- [x] `main.rs` keeps only CLI parsing and dispatch (coding-principles entry point rule).
- [x] Functions moved unchanged; functions over ~20 lines in these files are split in this
      stage where the split is mechanical.

Files: `src/tui/list_app.rs`, `src/tui/list_app/`, `src/tui/widgets/file_explorer.rs`,
       `src/tui/widgets/file_explorer/`, `src/tui/cleanup_app.rs`, `src/tui/cleanup_app/`,
       `src/main.rs`, `src/dispatch.rs`, `src/tui/mod.rs`, `src/tui/widgets/mod.rs`
Depends on: Stage 19

### Stage 22: Flaky timing assertions in silence removal perf tests (user request, in this MR)

Goal: CI runs `cargo test` including `tests/performance`, and three tests assert wall-clock
budgets of 1ms/1ms/100ms. `hundred_events_fast` failed once under load during Batch 5
verification, passes 3/3 alone. Make them deterministic without losing their intent.
Owner: implementer

- [x] `single_event_no_overhead`, `hundred_events_fast`, `ten_thousand_events_reasonable_time`
      in `tests/performance/silence_removal_test.rs`: replace absolute wall-clock budgets with a
      check that cannot flake on a loaded runner and still catches a regression to quadratic
      behavior (e.g. compare 10k vs 1k events scaling ratio with a generous bound, measured as
      the minimum over several runs, or count work done). Keep the correctness assertions.
- [x] Prove the new check catches a regression: temporarily make the transform quadratic,
      capture the failing assertion line, revert.
- [x] Run the performance suite 20 times in a row while `cargo build --release` runs in
      parallel, zero failures.

Files: `tests/performance/silence_removal_test.rs`
Depends on: Stage 20, Stage 21

### Stage 23: Stage 20/21 review fixes and Windows check (user request, in this MR)

Goal: fix every Stage 20/21 review finding, add a Windows compile check to CI.
Owner: implementer

- [x] `#[cfg(not(tarpaulin_include))]` on `dispatch::dispatch` (was covered by the excluded
      `main` before the split).
- [x] RED/GREEN: `repair` drops invalid-UTF-8 lines (deliberate since Stage 20, matches
      `diagnose`), test pins it: file with one invalid-UTF-8 line, repair removes exactly it,
      valid lines byte-identical. ADR note.
- [x] RED/GREEN: dir fsync warning is returned to the caller instead of `eprintln!` while a
      TUI may own the terminal. Design: a process-wide warning sink in `src/files/codec.rs`
      (`set_warning_sink(fn(&str))` / default stderr), the TUI app installs a sink that queues
      the message for its status line while it owns the terminal and restores stderr on exit.
      Keeps `atomic_write`'s signature for its many callers. Test: install a test sink, force
      a dir fsync failure, assert the sink got the message and nothing hit stderr.
- [x] `is_owned_by` becomes `pub(crate)`, test moved accordingly (inline test module if
      needed).
- [x] Comment on the non-unix `fresh_temp_dir` (fixed name, no ownership check, Windows is
      out of scope). ADR note on leftover `<tmp>/agr-clipboard` dirs from before the uid name.
- [x] RED/GREEN: `uncompressed_size` on a foreign multi-member `.cast.gz`: documented trailer
      limit (last member ISIZE), and `open_reader` behavior on trailing non-gzip bytes after
      the last member pinned by a test (error with path, no silent truncation).
- [x] Windows: fix `cargo check --target x86_64-pc-windows-msvc --all-targets` errors
      (`libc::getuid` in `tests/integration/clipboard_gzip_test.rs` without `#[cfg(unix)]`,
      unused `inode` in `src/recording.rs` on non-unix), then add a CI job
      `cargo check --target x86_64-pc-windows-msvc --all-targets` with `RUSTFLAGS=-D warnings`
      on `windows-latest` or cross from ubuntu.

Files: `src/dispatch.rs`, `src/asciicast/integrity.rs`, `src/files/codec.rs`,
       `src/clipboard/plain_copy.rs`, `src/recording.rs`, `src/tui/list_app/session_actions.rs`,
       `tests/integration/asciicast_test.rs`, `tests/integration/clipboard_gzip_test.rs`,
       `tests/integration/codec_test.rs`, `.github/workflows/ci.yml`
Depends on: Stage 20, Stage 21, Stage 22

## Dependencies

- Stage 1, Stage 2, Stage 3 are independent foundations.
- Stage 4 depends on Stage 2 (shares `config_test.rs`).
- Stage 5 depends on Stage 1, Stage 6 on Stage 1 and Stage 5.
- Stage 17 depends on Stage 1, Stage 2, Stage 3 (Batch 1 hardening, user request).
- Stage 20 depends on Stage 4, 5, 7, 15, 19 (Batch 4 review fixes).
- Stage 23 depends on Stage 20, 21, 22 (review fixes, Windows check).
- Stage 22 depends on Stage 20, Stage 21 (flaky timing perf tests).
- Stage 21 depends on Stage 19 (pure-move split of TUI files and main.rs).
- Stage 19 depends on Stage 17, Stage 18 (size limit for files this branch touched).
- Stage 18 depends on Stage 17 (Batch 1 pair review fixes, shares codec and storage tests).
- Stage 7 depends on Stage 1, Stage 2, Stage 3, Stage 17 (17 splits `storage_test.rs`). Stage 8 on Stage 7.
- Stage 9 depends on Stage 1, Stage 3, Stage 6, Stage 7.
- Stage 10 depends on Stage 2, Stage 9. Stage 11 on Stage 9. Stage 12 on Stage 4, 8, 11.
- Stage 13 depends on Stage 1, 7, 8. Stage 14 on Stage 1, 5, 6. Stage 15 on Stage 1.
- Stage 16 depends on everything user-facing.

```mermaid
flowchart LR
  S1 --> S5 --> S6
  S1 --> S15
  S2 --> S4
  S1 & S2 & S3 --> S17 --> S18 --> S7 --> S8
  S18 --> S5 & S15 & S4
  S6 & S7 --> S9
  S9 --> S10
  S9 --> S11 --> S12
  S4 & S8 --> S12
  S7 & S8 --> S13
  S5 & S6 --> S14
  S10 & S12 & S13 & S14 & S15 --> S16
```
Legend: arrows point from prerequisite to dependent stage.

## Progress

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | pending | |
| 2 | pending | |
| 3 | pending | |
| 4 | pending | |
| 5 | pending | |
| 6 | pending | |
| 7 | pending | |
| 8 | pending | |
| 9 | pending | |
| 10 | pending | |
| 11 | pending | |
| 12 | pending | |
| 13 | pending | |
| 14 | pending | |
| 15 | pending | |
| 16 | pending | |
