# Plan: Reduce Cognitive Complexity — tui/widgets

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
- [x] `build_rename_item_spans(input, cursor, selected_all, agent, size_str, theme) -> Vec<Span>` — extracted rename-mode inline editor spans (deepest nesting in closure)
- [x] `build_normal_item_spans(name, agent, size_str, is_locked, theme) -> Vec<Span>` — extracted normal list item spans with lock styling
- [x] `render_preview_panel(buf, area, preview_data, session_preview_data, has_backup, theme)` — extracted entire right-hand preview panel rendering
- [x] `build_preview_lines(name, agent, size, modified, path, lock_data, session_preview_data, has_backup, theme) -> Vec<Line>` — extracted preview line construction
- [x] `append_session_preview_lines(lines, duration, markers, styled_preview, modified, lock_data, has_backup, theme)` — extracted session-specific preview section (duration, markers, terminal snapshot)
- [x] Added `PreviewData` type alias for the 6-tuple to improve readability

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `src/tui/widgets/file_explorer.rs` — extracted 5 free functions and 1 type alias from `FileExplorerWidget::render()`, reducing 5+ nesting levels to manageable depth

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `FileExplorerWidget::render()` | 79 | `build_rename_item_spans()`, `build_normal_item_spans()`, `render_preview_panel()`, `build_preview_lines()`, `append_session_preview_lines()` | < 15 |
