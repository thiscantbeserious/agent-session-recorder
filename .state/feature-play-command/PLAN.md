# Plan: Play Command

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. Should the play command validate `.cast` extension or accept any file? Other commands warn but proceed - follow same pattern.
2. How should the command handle player errors (e.g., corrupt file)? The native player already has error handling - let it propagate naturally.

## Stages

### Stage 1: CLI Command Definition

Goal: Add the `play` subcommand to the CLI with proper help text

- [x] Add `Play { file: String }` variant to `Commands` enum in `src/cli.rs`
- [x] Add appropriate help text and examples matching other commands
- [x] Verify `agr play --help` displays correctly

Files: `src/cli.rs`

Considerations:
- Match the style of existing commands (analyze, marker, optimize)
- Include example in long_about showing `agr play claude/session.cast`

### Stage 2: Command Handler

Goal: Implement the play command handler that invokes the native player

- [x] Create `src/commands/play.rs` with `handle(file: &str)` function
- [x] Use `resolve_file_path()` for path resolution
- [x] Add existence check with helpful error message
- [x] Add `.cast` extension warning (match other commands)
- [x] Call `player::play_session(path)`
- [x] Export module in `src/commands/mod.rs`

Files: `src/commands/play.rs`, `src/commands/mod.rs`

Considerations:
- Follow the exact pattern from `analyze.rs` for consistency
- Error message should include hint about `agr list`
- Player handles its own cleanup (terminal restore, etc.)

### Stage 3: Main Dispatch

Goal: Wire up the command in main.rs

- [x] Add match arm for `Commands::Play` in `main.rs`
- [x] Call `commands::play::handle()`

Files: `src/main.rs`

Considerations:
- Simple dispatch, no special handling needed
- Player manages terminal state internally

### Stage 4: Integration Tests

Goal: Verify command works end-to-end

- [x] Test: `agr play --help` shows usage
- [x] Test: `agr play nonexistent.cast` shows error with hint
- [x] Test: `agr play` without arguments shows usage/error
- [x] Test: Path resolution works (short format `agent/file.cast`)

Files: `tests/integration/play_test.rs` or extend existing CLI tests

Considerations:
- Cannot easily test actual playback (requires terminal interaction)
- Focus on argument parsing, path resolution, error handling
- Use existing test helpers for temp files

### Stage 5: Documentation Updates

Goal: Update README with play command and player capabilities

- [x] Add `agr play` to quick start section in README
- [x] Document native player controls in a new section or table:
  - `q` / `Esc` - Quit
  - `Space` - Pause/resume
  - `+` / `-` - Speed up/down
  - `<` / `>` - Seek back/forward 5s
  - `m` - Jump to next marker
  - `?` - Show help overlay
- [x] Update command reference if auto-generated (run `cargo xtask gen-docs`)
- [x] Add play command to wiki if applicable

Files: `README.md`, `docs/COMMANDS.md` (via xtask)

Considerations:
- Keep quick start concise - just show the command exists
- Player controls can be a dedicated subsection
- Verify controls by checking `src/player/native.rs` help overlay

## Dependencies

```
Stage 1 ──> Stage 2 ──> Stage 3 ──> Stage 4
                                      │
                                      v
                                   Stage 5
```

- Stage 2 depends on Stage 1 (needs CLI definition)
- Stage 3 depends on Stage 2 (needs handler)
- Stage 4 depends on Stage 3 (needs working command)
- Stage 5 can start after Stage 3 but should verify with Stage 4

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | complete | CLI command definition added to src/cli.rs |
| 2 | complete | Handler created in src/commands/play.rs |
| 3 | complete | Match arm added in main.rs |
| 4 | complete | Tests in tests/integration/play_test.rs and main.rs |
| 5 | complete | README updated, docs regenerated |
