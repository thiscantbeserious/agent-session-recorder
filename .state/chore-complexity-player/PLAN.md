# Plan: Reduce Cognitive Complexity — player

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `render_row(output, row, col_offset, view_cols, is_highlighted)` — shared row renderer dispatching to cells or empty-row path
- [x] `render_row_cells(output, cells, col_offset, view_cols, is_highlighted)` — column loop with per-cell style tracking
- [x] `render_cell(output, cell, is_highlighted, current_style, in_highlight_style)` — single cell ANSI style application
- [x] `render_space_past_content(output, is_highlighted, current_style)` — space fill with style reset for columns beyond content
- [x] `render_empty_row(output, view_cols, is_highlighted)` — empty row fill with optional highlight reset

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `src/player/render/viewport.rs` — extracted 5 helper functions shared between `render_viewport()` and `render_single_line()`, eliminating duplicate column-loop logic

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `render_viewport()` | 44 | `render_row()`, `render_row_cells()`, `render_cell()`, `render_space_past_content()`, `render_empty_row()` | < 15 |
| `render_single_line()` | 27 | (same shared helpers) | < 15 |
