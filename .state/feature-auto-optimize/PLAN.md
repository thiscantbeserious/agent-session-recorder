# Plan: Auto-Optimize After Recording

References: ADR.md, REQUIREMENTS.md

Every stage is TDD (RED test first, GREEN minimal code, REFACTOR), ends with `cargo fmt`,
`cargo clippy -- -D warnings` and `cargo test`. No trivial closures where a method reference
works (SonarCloud rust:S1612). Files stay under ~400 lines, functions ~20 lines, nesting max 3.
New tests go in `tests/integration/` (binary-only code like `src/commands/` keeps inline tests,
as `compress_offer.rs` does). No new dependencies.

## Open Questions

None. All resolved by the user on 2026-11-06 (ADR Decision History 3): interrupted recordings
skip optimize, auto mode keeps no `.bak`, no size guard (follow-up issue for streaming).

## Stages

### Stage 1: optimize_file with backup flag, remove_silence, threshold guard

Goal: one transform path in `transform_ops.rs`. Private `optimize_file(path, backup: bool)`,
`apply_transforms` (backup, unchanged) and new public `remove_silence` (no backup) wrap it. A bad
header threshold returns an error instead of panicking. No duplicated transform logic.
Owner: implementer

- [x] RED: `remove_silence` clamps a 5s gap to 2.0s, header `idle_time_limit: 1.0` clamps to
      1.0s, `.cast.gz` stays gzip.
- [x] RED: `remove_silence` creates no `.x.cast.bak` / `.x.cast.gz.bak`, result has
      `backup_path: None`, `backup_created: false`.
- [x] RED: an existing (orphaned) `.x.cast.bak` is left byte-identical by `remove_silence`.
- [x] RED: `apply_transforms` still creates the `.bak` (existing tests stay green unchanged,
      this is the TUI path).
- [x] RED: header `idle_time_limit: 0.0` and `-1.0` make both wrappers return `Err` (today a
      panic in `SilenceRemoval::new`), file byte-identical, no `.bak`.
- [x] GREEN: move the body into `optimize_file`, `create_backup` behind `if backup`, threshold
      check (positive and finite) right after parse. Export `remove_silence` from `asciicast`.
- [x] `// ponytail:` comment on `optimize_file`: whole file parsed in memory (~2-3x plain size),
      streaming optimize is the upgrade path (follow-up issue, ADR section 5).

Files: `src/asciicast/transform_ops.rs`, `src/asciicast/mod.rs`, `tests/integration/transform_ops_test.rs`
Depends on: none

### Stage 2: Config v5 with recording.auto_optimize

Goal: `[recording] auto_optimize: bool`, `Config::default()` and serde default `true`,
versioned migration v5.
Owner: implementer

- [x] RED: `migrate_config("config_version = 4\n[recording]\n")` yields `auto_optimize = true`,
      `config_version = 5`, `"recording.auto_optimize"` in `added_fields`.
- [x] RED: an existing `auto_optimize = false` is kept and not in `added_fields`.
- [x] RED: running the migration on its own output changes nothing (`has_changes() == false`).
- [x] RED: a config without the key deserializes with `auto_optimize == true`.
- [x] RED: `auto_optimize = "yes"` fails to parse with an error naming the field.
- [x] GREEN: field in `RecordingConfig` with `#[serde(default = "default_auto_optimize")]`,
      `v5.rs` (empty `migrate`, like v4), registry entry, `CURRENT_VERSION = 5`.
- [x] `FieldDoc` in `docs/fields.rs` ("Remove idle time from each recording after it ends (no backup
      is kept)", default `true`), example in `docs/markdown.rs`.
- [x] Update the default-config snapshot (`cargo insta review`), check only the new key and the
      version changed.

Files: `src/config/types.rs`, `src/config/migrate/migrations/v5.rs`, `src/config/migrate/migrations/mod.rs`, `src/config/migrate/mod.rs`, `src/config/docs/fields.rs`, `src/config/docs/markdown.rs`, `tests/integration/snapshots/agr__config__migrate__tests__complete_default_config_snapshot.snap`
Depends on: none

Considerations:
- Sync (#166) renumbers to v6 after this merges, note it in the PR description.

### Stage 3: optimize_recording in recording/finalize.rs

Goal: `pub fn optimize_recording(path: &Path, config: &Config) -> Option<String>`, returns the
warning line, prints nothing, never creates a backup.
Owner: implementer

- [x] RED: `auto_optimize = false` leaves the file byte-identical, returns `None`.
- [x] RED: `auto_optimize = true`, a 5s gap is clamped to 2.0s, no `.x.cast.bak`, returns `None`.
- [x] RED: unparsable file stays byte-identical, returns
      `Some(format!("Auto-optimize failed, kept original: {e:#}"))` (`{e:#}` like
      `finalize_recording`, so the context chain is in the one line).
- [x] RED: header `idle_time_limit: 0` returns `Some("Auto-optimize failed, ...")`, file
      byte-identical (guard from Stage 1 reaches the recording path).
- [x] RED: missing file returns `None` (path recovery already warned).
- [x] RED (interplay, ADR section 3): `optimize_recording` then `finalize_recording` with gzip
      leaves only `x.cast.gz` (optimized duration), no `x.cast`, no `.x.cast.bak`, no
      `.x.cast.gz.bak`.
- [x] RED: same with `compression = "none"` leaves only `x.cast` (optimized), no `.x.cast.bak`.
- [x] RED: unparsable file then `finalize_recording` with gzip still yields `x.cast.gz` with the
      original content.
- [x] GREEN: implement with `asciicast::remove_silence`.

Files: `src/recording/finalize.rs`, `src/recording.rs`, `tests/integration/recording_test.rs`
Depends on: Stage 1, Stage 2

Considerations:
- `src/recording.rs` only gets the `pub use finalize::optimize_recording` re-export here.

### Stage 4: Hook into Recorder::finish and e2e

Goal: `finish` runs optimize after path recovery and before compression, skipped when
interrupted, warning printed with `  \u{26a0} ` prefix.
Owner: implementer

- [ ] In `finish`: `if !self.guard.is_interrupted() { if let Some(w) = optimize_recording(..)
      { eprintln!(..) } }` between `resolve_actual_path` and `finalize_recording`. Keep `finish`
      ~20 lines (extract `maybe_optimize` if needed), `recording.rs` under 400 lines.
- [ ] Update the `finish` doc comment to name the optimize step.
- [ ] e2e: with `auto_optimize = true`, record `sh -c 'echo a; sleep 3; echo b'`, assert the
      `.cast.gz` duration is under 3s and no `.<name>.cast.gz.bak` / `.<name>.cast.bak` exists.
- [ ] e2e: with `auto_optimize = false`, the duration stays at 3s or more.
- [ ] Full e2e suite passes (CI config has no key, so default yes is active everywhere).

Files: `src/recording.rs`, `tests/e2e/recording.sh`
Depends on: Stage 3

### Stage 5: Post-migrate prompt registry

Goal: `src/config/migrate/prompts.rs` with:

```rust
pub struct PostMigratePrompt { pub key: &'static str, pub question: &'static str, pub default: bool }
pub const PROMPTS: &[PostMigratePrompt] = &[PostMigratePrompt {
    key: "recording.auto_optimize",
    question: "Optimize recordings automatically after each session?",
    default: true,
}];
pub fn apply_prompts(result: &MigrateResult, ask: impl FnMut(&PostMigratePrompt) -> Result<bool>) -> Result<String>;
pub fn apply_prompt_list(result: &MigrateResult, prompts: &[PostMigratePrompt], ask: impl FnMut(&PostMigratePrompt) -> Result<bool>) -> Result<String>;
```

`apply_prompts` is `apply_prompt_list(result, PROMPTS, ask)`. Both parse `result.content` as
`toml_edit::DocumentMut`, return the new content string and never touch the disk (writing stays
in `handle_migrate`, Stage 6).
Owner: implementer

- [x] RED: `"recording.auto_optimize"` in `added_fields`, `ask` returns false, content has
      `auto_optimize = false`, everything else unchanged (comments kept).
- [x] RED: `ask` returns true keeps `auto_optimize = true`.
- [x] RED: key not in `added_fields` (already in the file), `ask` is never called, content
      equals `result.content`.
- [x] RED: a test-only prompt list with two entries asks both in order and writes both (runner
      takes the list as a parameter, `apply_prompts` passes `PROMPTS`).
- [x] RED: `ask` error is returned as `Err` (no content returned, so nothing can be written).
- [x] RED: fresh config (`migrate_config("")`, whole `[recording]` section added) also asks,
      `added_fields` then holds `"recording.auto_optimize"` via `track_added_fields`.
- [x] GREEN: dotted key walk with `toml_edit`, set `value(bool)`. `pub mod prompts;` plus
      `pub use prompts::{apply_prompt_list, apply_prompts, PostMigratePrompt, PROMPTS};`
      (explicit, not a wildcard, Sonar S2208) in `src/config/migrate/mod.rs` (`config` re-exports `migrate::*`).

Files: `src/config/migrate/prompts.rs`, `src/config/migrate/mod.rs`, `tests/integration/config_prompts_test.rs`, `tests/integration.rs`
Depends on: Stage 2

### Stage 6: Ask helper and handle_migrate wiring

Goal: `agr config migrate` asks once after confirmation, `--yes` and no TTY write the default.
Owner: implementer

- [x] RED (inline): `parse_answer("", true) == true`, `"n"`/`"no"` false, `"y"`/`"YES"` true,
      `"x"` keeps the default, `parse_answer("", false) == false`.
- [x] GREEN: `ask_with_default(message, default, auto_confirm)` in `prompt.rs`: auto_confirm or
      no TTY returns `default` silently, else prints `[Y/n]`/`[y/N]` and uses `parse_answer`.
      `should_proceed` unchanged.
- [x] Wire `apply_prompts` into `handle_migrate` and `handle_new_config_creation` after the
      confirmation, before `atomic_write`, so the file is written once.
- [x] Manual check: `HOME=$(mktemp -d) agr config migrate` asks the question after the create
      prompt, Enter writes `auto_optimize = true`, second run says "already up to date".
- [x] Manual check: `agr config migrate --yes` on a v4 config writes `true` without asking.

Files: `src/commands/prompt.rs`, `src/commands/config.rs`
Depends on: Stage 5

### Stage 7: README

Goal: README describes `auto_optimize`, the default, the post-migrate question, that auto mode
keeps no backup (TUI optimize does) and the flow order (optimize, compress, stop banner, auto-analyze, storage warning).
Owner: implementer

- [ ] `[recording]` example and a short "Auto-optimize" section, mention `agr optimize` and
      TUI optimize stay manual alternatives.

Files: `README.md`
Depends on: Stage 4, Stage 6
