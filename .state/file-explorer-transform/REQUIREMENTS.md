# Requirements: TUI Transform Integration with Context Menu

## Problem Statement
The silence removal transform (`agr transform --remove-silence`) exists only as a CLI command. Users browsing recordings in the TUI file explorer must exit, run the transform command manually, then re-enter the TUI. This disrupts the workflow for users who want to quickly clean up recordings before playback.

Additionally, the current "Enter" key behavior (immediate playback) doesn't scale well as more actions are added. A context menu approach provides better discoverability and a more cohesive UX.

## Desired Outcome
Users can apply transforms directly from the TUI file explorer, with automatic backup and restore capability. The Enter key opens a context menu for all file actions, while direct keyboard shortcuts remain available for power users.

## Scope

### In Scope

**Context Menu (Enter Key Refactor):**
- Refactor Enter key to open a context menu instead of immediate playback
- Context menu options:
  - Play (p) - play session with asciinema
  - Transform (t) - apply transforms to recording
  - Restore (r) - restore from backup (disabled/hidden if no backup exists)
  - Delete (d) - delete session (with confirmation)
  - Add Marker (m) - add marker to recording
  - Cancel (Esc) - close menu
- Menu renders as modal overlay (similar to existing Help and ConfirmDelete modals)

**Direct Keyboard Shortcuts (Power User Access):**
- Keep `p` as direct shortcut for play (new, replaces Enter behavior)
- Keep `t` as direct shortcut for transform
- Keep `r` as direct shortcut for restore from backup
- Keep `d` as direct shortcut for delete
- Keep `m` as direct shortcut for marker
- All shortcuts bypass context menu for faster workflow

**Backup File Display Behavior:**
- `.bak` files should NOT appear in the file list (left panel) - keeps list uncluttered
- Preview panel (right) should show "Backup available" indicator when `.bak` file exists for selected recording
- This allows users to know restore is available without polluting the file list

**Transform Feature:**
- Apply silence removal with default threshold (2.0s or header's `idle_time_limit`)
- Modify file in-place (overwrite original)
- Create backup of original file (`<filename>.cast.bak`)
- Add restore command (`r` key) to recover from backup
- Show status feedback (success with time savings, or error message)
- Design for future transform extensibility (apply ALL transforms automatically)

### Out of Scope
- Per-transform customization (user picks which transforms) - intentionally excluded
- Custom threshold input in TUI
- Multiple transform types beyond silence removal (future work)
- Batch transforms on multi-selected files (future work)
- Performance test CI failure (tracked separately - see Related Issues)

## Acceptance Criteria

**Context Menu:**
- [ ] Pressing Enter opens context menu modal
- [ ] Context menu shows: Play (p), Transform (t), Restore (r), Delete (d), Add Marker (m), Cancel (Esc)
- [ ] Restore option is disabled/hidden when no backup exists for selected file
- [ ] Pressing corresponding key in menu executes action
- [ ] Esc closes menu without action
- [ ] Clicking outside menu area closes it (if mouse support exists)

**Direct Shortcuts:**
- [ ] Pressing `p` in normal mode plays session directly (no menu)
- [ ] Pressing `t` in normal mode applies transform directly
- [ ] Pressing `r` in normal mode restores from backup (if exists)
- [ ] Pressing `d` in normal mode shows delete confirmation
- [ ] Pressing `m` in normal mode triggers marker action

**Transform:**
- [ ] Transform applies silence removal with correct threshold
- [ ] Original file backed up to `<filename>.cast.bak` before modification
- [ ] Success message shows time savings (e.g., "Removed 5m 32s of silence")
- [ ] Error message displayed if transform fails
- [ ] Pressing `r` restores from backup if backup exists
- [ ] Error message if no backup exists for restore

**Backup Display:**
- [ ] `.bak` files are excluded from the file list (left panel)
- [ ] Preview panel shows "Backup available" indicator when backup exists for selected file
- [ ] Indicator updates when selection changes

**UI Updates:**
- [ ] Help modal (`?`) shows updated keybindings including context menu
- [ ] Footer hints update to reflect new shortcuts
- [ ] Context menu styled consistently with existing modals

## Constraints
- Must not block UI during transform (show loading indicator for large files)
- Backup file must be in same directory as original
- Only one backup per file (subsequent transforms overwrite backup)
- Context menu should be keyboard-navigable (arrow keys + enter, or single-key shortcuts)

## Related Issues
- Performance test CI failure - separate bug, not part of this feature scope

## Context
- Silence removal CLI merged in PR #63 (`agr transform --remove-silence`)
- Transform trait: `src/asciicast/transform.rs`
- SilenceRemoval: `src/asciicast/silence_removal.rs`
- File explorer widget: `src/tui/widgets/file_explorer.rs`
- List app (key handling): `src/tui/list_app.rs`
- Existing modal examples: Help modal, ConfirmDelete modal in `list_app.rs`
- Current Enter behavior: calls `play_session()` directly

---
**Sign-off:** Approved by user (2026-01-28)
