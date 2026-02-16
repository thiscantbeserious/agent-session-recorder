# Requirements: Refresh file size for locked visible items

## Problem Statement
The TUI refresh tick updates lock information but does not refresh file size, so file sizes can stay stale while recordings are active.

## Desired Outcome
During the periodic refresh tick, the file explorer updates file sizes for visible items that are being refreshed for locks so sizes reflect ongoing changes.

## Scope
### In Scope
- Update file size metadata for visible items that currently have `lock_info` set during the refresh tick.
- Keep the existing refresh cadence and behavior (no new timers or intervals).

### Out of Scope
- Updating size for all visible items or all items in the list.
- Changes to sorting, filtering, or any other TUI behavior.

## Acceptance Criteria
- [ ] On each refresh tick, visible items with `lock_info.is_some()` have their `size` refreshed from filesystem metadata.
- [ ] If metadata read fails or the file is missing, the existing size value remains unchanged and the refresh continues without error.

## Constraints
- Minimal change; reuse the existing lock refresh pathway.

## Context
- Refresh tick currently calls `refresh_visible_locks()` in the file explorer and only for visible items with locks.

---
**Sign-off:** Approved by user
