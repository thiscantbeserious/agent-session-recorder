# Plan: Reduce Cognitive Complexity — tui/cleanup_app

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `parse_glob_pattern(pattern: &str) -> (Option<&str>, &str)` — extracted agent/file pattern parsing (lines 192-198) into free function
- [x] `matches_glob_pattern(agent, name, agent_filter, file_pattern) -> bool` — extracted glob match logic (lines 225-229) into free function

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `src/tui/cleanup_app.rs` — extracted `parse_glob_pattern()` and `matches_glob_pattern()` from `select_by_glob()`, reducing nesting in the main loop body

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `CleanupApp::select_by_glob()` | 16 | `parse_glob_pattern()`, `matches_glob_pattern()` | < 15 |
