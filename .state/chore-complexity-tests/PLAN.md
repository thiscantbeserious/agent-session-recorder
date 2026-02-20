# Plan: Reduce Cognitive Complexity — tests

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `render_snapshot_row(buffer, buf_row, col_offset, view_cols) -> String` — extracted row content rendering with cell/space fallback into free function
- [x] `render_viewport_row(buffer, buf_row, col_offset, view_cols, is_highlighted) -> String` — extracted full viewport row including highlight markers (prefix/suffix) into free function

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `tests/integration/snapshot_player_test.rs` — extracted `render_snapshot_row()` and `render_viewport_row()` from `render_viewport_snapshot()`, reducing triple nesting to single loop

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `render_viewport_snapshot()` | 21 | `render_snapshot_row()`, `render_viewport_row()` | < 15 |
