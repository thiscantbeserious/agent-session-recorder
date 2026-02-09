# ADR: Add Rename Action to `agr ls` TUI

## Status
Accepted

## Context

The `agr ls` TUI provides an interactive file explorer for managing session recordings. Currently it supports Play, Copy, Optimize, Analyze, Restore, Delete, and Add Marker actions. Two usability improvements are needed:

1. **Rename support**: Users cannot rename recordings from within the TUI. They must exit, manually rename the `.cast` file on disk, and re-enter. This is friction-heavy, especially after AI analysis which often renames files automatically.

2. **Menu cleanup**: The AddMarker action is a placeholder ("coming soon!") that occupies a shortcut key (`m`) and menu slot. The Optimize hint line adds vertical noise. The context menu ordering does not follow a clear intent-based grouping.

### Technical Context

The codebase already has:
- `FileExplorer::update_item_path(old_path, new_path)` for in-place path/name updates after rename
- `Mode::Search` with inline text input (Backspace/Enter/Esc) as a pattern for text input modes
- `Mode::ConfirmDelete` and `Mode::ConfirmUnlock` as patterns for modal confirmation flows
- `preview_cache.invalidate()` for cache cleanup after file changes
- `backup_path_for()` which appends `.bak` to the original path (e.g., `session.cast.bak`)
- `ContextMenuItem` enum with `ALL` const array and `label()`/`shortcut()` methods

### Requirements Summary

1. Add `Rename` action with `r` shortcut key (Normal mode + context menu)
2. Remove `AddMarker` action entirely (enum variant, menu, shortcut, method)
3. Remove silence hint line from context menu Optimize entry
4. Reorder context menu by intent grouping
5. Interactive inline text input for entering new filename

## Options Considered

### Option A: Inline Status Bar Input (like Search mode)

Add `Mode::RenameInput` that renders a text input in the status bar area, following the same pattern as `Mode::Search`.

**How it works:**
- User presses `r` in Normal mode (or selects Rename in context menu)
- Status bar shows: `Rename: current_stem_` with a cursor
- `rename_input: String` field on `ListApp` pre-filled with current filename stem
- Typing replaces characters, Backspace deletes, Enter confirms, Esc cancels
- On Enter: validate, rename filesystem files, update explorer, invalidate cache

**Pros:**
- Follows the established Search mode pattern exactly (same key handling structure)
- Minimal new rendering code; reuses status bar area
- Lightweight and fast; no modal overlay to render/clear
- Users already understand this interaction from Search mode

**Cons:**
- Status bar is a single line; long filenames may truncate
- Less visually prominent; user might not notice the mode change
- No room for error messages inline (must use status message after)

### Option B: Centered Modal Dialog (like ConfirmDelete)

Add `Mode::RenameInput` that renders a centered modal dialog with a text input field.

**How it works:**
- User presses `r`, a centered modal appears with:
  - Title: "Rename Session"
  - Current name display
  - Text input field with cursor
  - Footer: "Enter: confirm | Esc: cancel"
- Same keyboard handling as Option A, but rendered in a modal

**Pros:**
- More visually prominent; impossible to miss
- Room for current name, input field, and inline error messages
- Consistent with other modal interactions (ConfirmDelete, OptimizeResult)

**Cons:**
- More rendering code (new modal layout, Clear behind it, border styling)
- Overkill for a simple text input; modals in this TUI are for confirmations
- Modals in the codebase are read-only or y/n; a text-input modal would be a new pattern

## Decision

**Option A: Inline Status Bar Input**

Rationale:
1. **Pattern consistency**: The TUI already has a well-tested inline text input mode (Search). Rename is structurally identical -- type text, Enter to confirm, Esc to cancel. Reusing this pattern minimizes new code and cognitive load.
2. **Modal purpose mismatch**: Existing modals (ConfirmDelete, ConfirmUnlock, OptimizeResult) display information and accept y/n or Enter/Esc. They do not accept freeform text. Introducing a text-input modal would be a new pattern that doesn't match existing usage.
3. **Simplicity**: The status bar line is sufficient for filename stems (which are typically short). Error feedback goes to the status message after the operation, consistent with how Copy, Optimize, and Delete report results.

### Rename Behavior Specification

- **Input**: Pre-filled with the current filename stem (without `.cast` extension), **entire stem selected** (first keystroke replaces all — natural for replacing)
- **Input validation (real-time, as user types)**:
  - Reject characters in `INVALID_CHARS` from `src/files/filename.rs`: `['/', '\\', ':', '*', '?', '"', '<', '>', '|']`
  - Enforce 255 char max length via `filename::validate_length()` — silently refuse chars beyond limit
- **Validation on Enter**:
  - Empty input: reject, show error status message
  - Same as current name: no-op, return to Normal mode
  - Target file already exists: reject with "File already exists" message
- **Filesystem operations**: Generalized `rename_file(old_path, new_stem)` in `src/files/filename.rs`, reusing `INVALID_CHARS`, `validate_length()`, and `backup::backup_path_for()`. Extension-agnostic — preserves original extension, works for any file type.
  1. Rename `old_stem.{ext}` to `new_stem.{ext}` (same directory, same extension)
  2. If backup exists (via `backup_path_for()`), rename backup too (best-effort)
  3. Return new path for the caller (TUI) to handle cache/explorer updates
- **TUI integration** (`list_app.rs`): After successful rename, invalidate preview cache + update explorer via `update_item_path()`
- **Lock check**: If file is locked, show ConfirmUnlock first (same as other actions)

### Context Menu Reorder

New order grouped by intent:

| # | Item | Shortcut | Intent |
|---|------|----------|--------|
| 1 | Play | p | View |
| 2 | Copy | c | Export |
| 3 | Rename | r | Modify (name) |
| 4 | Optimize | t | Modify (content) |
| 5 | Analyze | a | Modify (content) |
| 6 | Restore | -- | Revert |
| 7 | Delete | d | Destructive |

Restore loses its `r` shortcut (now used by Rename) and becomes menu-only. This is acceptable because Restore is an infrequent recovery action.

## Consequences

### What becomes easier
- Renaming recordings from within the TUI (no need to exit)
- Context menu is cleaner (no placeholder AddMarker, no silence hint noise)
- Menu ordering follows predictable intent grouping (view, export, modify, revert, destroy)

### What becomes harder
- Restore is no longer accessible via a direct shortcut key from Normal mode
- Users accustomed to `m` for marker must unlearn the muscle memory (but the feature was a placeholder anyway)

### Technical debt considerations
- `rename_input` field and `rename_selected_all` flag added to `ListApp` struct (small increase in state)
- Rename function added to existing `src/files/filename.rs` (reuses `INVALID_CHARS`, `validate_length()`, `backup_path_for()`)
- Seven snapshot tests need regeneration (context menu + help modal)
- Two unit tests updated (`context_menu_has_seven_items` stays at 7, `context_menu_item_order` changes)

### Risks
- Race condition: another process could create the target file between existence check and rename. On Unix, `fs::rename()` silently **overwrites** the target (POSIX semantics), so the check-then-act is inherently racy. Acceptable for a single-user TUI tool — not a multi-tenant file server. Noted for honesty; no mitigation needed.

## Decision History

1. User decided: `r` shortcut moves from Restore to Rename
2. User decided: Remove Add marker entirely
3. User decided: Remove silence hint from context menu
4. User decided: Reorder context menu by intent grouping
5. Architect decided: Inline status bar input (Option A) over modal dialog (Option B)
6. User decided: Select entire stem on pre-fill (first keystroke replaces all)
7. User decided: Reject invalid chars as typed (using `INVALID_CHARS` from `filename.rs`)
8. User decided: Enforce 255 char limit during typing (not just on submit)
9. User decided: Rename function goes in existing `src/files/filename.rs` — no new module, reuses INVALID_CHARS/validate_length already there
