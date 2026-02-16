# ADR: Refresh visible locked item metadata on tick

## Status
Accepted

## Context
The TUI refresh tick currently updates lock information for visible items that have a lock. File sizes can become stale while a recording is ongoing. We need a minimal change that refreshes size alongside lock data, limited to the same locked visible items during the tick.

## Options Considered

### Option 1: Extend the existing refresh method and rename it
- Pros: Minimal change, reuses existing refresh pathway, keeps scope tight to locked visible items.
- Cons: Slightly broader responsibility for the renamed method.

### Option 2: Add a new method and keep the old one
- Pros: Clear separation between lock-only and metadata refresh.
- Cons: Additional API surface and minor overhead for a tiny feature.

## Decision
Choose Option 1: rename the method to `refresh_visible_item_metadata` and update it to refresh file size for visible items with `lock_info.is_some()` while keeping the same tick cadence.

## Consequences
- Easier: File sizes stay accurate for active recordings without extra scans.
- Harder: The method name change requires updating call sites.
- Follow-ups: None; keep scope minimal.

## Decision History

Decisions made with user during design (numbered, 1-2 sentences each):

1. Use the minimal-change approach (Option A) and rename the method to `refresh_visible_item_metadata`.
2. Limit refresh to visible items with locks, matching current lock refresh behavior.
