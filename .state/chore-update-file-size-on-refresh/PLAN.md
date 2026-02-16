# Plan: Refresh visible locked item metadata on tick

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. None.

## Stages

### Stage 1: Update metadata refresh method

Goal: Rename the lock refresh method and update it to also refresh file size for visible locked items.

- [x] Rename `refresh_visible_locks` to `refresh_visible_item_metadata`.
- [x] In the renamed method, keep the lock-only condition and add size refresh from filesystem metadata.
- [x] Update call sites in the tick refresh to use the renamed method.

Files: `src/tui/widgets/file_explorer.rs`, `src/tui/app/shared_state.rs`

Considerations:
- If metadata read fails, keep the existing size and continue.
- Only apply to visible items with `lock_info.is_some()`.

## Dependencies

- Stage 1 is standalone.

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | completed | Renamed method and updated lock-filtered size refresh/call sites. |
