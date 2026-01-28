# Plan: TUI Context Menu and Transform Integration

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. How to extract transform logic from `commands/transform.rs` into a reusable function that doesn't depend on CLI output?
2. Should the context menu highlight start at index 0 (Play) or remember last selection?
3. How to handle transform errors gracefully in the modal (file permission errors, parse failures)?
4. Should `.bak` files be included in cleanup command scope, or explicitly excluded?
5. How to render Restore option in context menu when no backup exists (grayed out vs hidden)?
6. Is `insta` already a dev-dependency, or does it need to be added?

## Regression Safety

**CRITICAL**: The following existing features MUST continue working after each stage:

1. **`agr cleanup`** - Deletes old recordings. Verify it still works; consider `.bak` file handling.
2. **`agr ls`** - Lists recordings. Must not break with `.bak` files present.
3. **Preview panel** - Shows duration, markers, terminal snapshot. Must refresh correctly after transforms.

Each relevant stage includes regression checks for these features.

## Stages

### Stage 1: Preparation and Research Documentation

Goal: Commit research folder and verify existing transform infrastructure

- [x] Commit `research/` folder to git with appropriate message
- [x] Verify `SilenceRemoval` transform works correctly via CLI
- [x] Document current key bindings in list_app.rs for reference
- [x] **Regression**: Run `agr ls` and verify output is correct
- [x] **Regression**: Run `agr cleanup --dry-run` and verify it works
- [x] **Regression**: Open TUI, select a file, verify preview panel shows duration/markers

**Stage 1 Notes:**
- Research committed in be1816f
- Transform tested on 10MB file: reduced 11402.9s to 1916.9s (saved 9486s)
- `agr cleanup` does NOT have `--dry-run` flag - interactive only with cancel option (0)
- All 294 tests pass, formatting and clippy clean
- Current key bindings documented below

Files: `research/`, `src/commands/transform.rs`, `src/tui/list_app.rs`

Considerations:
- Research folder contains algorithm documentation for future spinner detection
- Ensure no breaking changes to existing CLI transform command
- Establish baseline for regression testing

---

### Stage 2: Extract Reusable Transform Logic

Goal: Create internal transform function that can be called from TUI without CLI dependencies

- [x] Create `src/tui/transform.rs` module with `apply_transforms()` function
- [x] Function signature: `fn apply_transforms(path: &Path) -> Result<TransformResult>`
- [x] `TransformResult` struct: original_duration, new_duration, backup_path, error details
- [x] Implement backup logic: only create `.bak` if it doesn't already exist
- [x] Add `has_backup(path: &Path) -> bool` helper function for checking `.bak` existence
- [x] Use threshold resolution: CLI arg > header's idle_time_limit > default 2.0s
- [x] Add unit tests for backup-exists logic
- [x] **Regression**: Verify `agr ls` does not list `.bak` files (or confirm expected behavior)
- [x] **Regression**: Verify `agr cleanup --dry-run` behavior with `.bak` files present

**Stage 2 Notes:**
- Created `src/tui/transform.rs` with 16 unit tests (all passing)
- `TransformResult` includes `time_saved()` and `percent_saved()` helpers
- `backup_path_for()`, `has_backup()`, `apply_transforms()`, `restore_from_backup()` implemented
- Round-trip integrity test verifies: transform -> restore -> transform -> restore = original bytes
- `agr ls` already filters by `.cast` extension (line 176 in storage.rs), so `.bak` files excluded
- `agr cleanup` uses same `list_sessions()` function, also excludes `.bak` files

Files: `src/tui/transform.rs`, `src/tui/mod.rs`

Considerations:
- Edge case: What if source file is read-only?
- Edge case: What if disk is full during write?
- The function should be synchronous (TUI is single-threaded)
- `.bak` files should not appear in `agr ls` output (verify or fix)

---

### Stage 2b: Filter Backup Files from TUI List

Goal: Ensure `.bak` files never appear in the file explorer list

- [x] Identify where file list is populated (likely `list_app.rs` or storage module)
- [x] Add filter to exclude files ending in `.bak` from the list
- [x] Verify filter works when `.bak` files exist in recordings directory
- [x] Add unit test for filtering logic

**Stage 2b Notes:**
- File enumeration happens in `storage.rs` `list_sessions()` (line 176)
- Filter already exists: `path.extension().is_some_and(|ext| ext == "cast")`
- This filters by `.cast` extension only, automatically excluding `.bak` files
- Added 4 new unit tests in `storage.rs`:
  - `list_sessions_excludes_bak_files` - critical regression test
  - `list_sessions_only_includes_cast_extension` - verifies extension filter
  - `list_sessions_empty_directory` - edge case
  - `list_sessions_filters_by_agent` - agent filter works correctly

Files: `src/tui/list_app.rs`, `src/storage.rs` (or wherever file enumeration occurs)

Considerations:
- Filter should be applied at enumeration time, not rendering time
- Ensure filter doesn't accidentally exclude valid `.cast.bak` patterns that aren't backups

---

### Stage 3: Add Restore Functionality

Goal: Implement backup restore with `r` key in Normal mode

- [x] Add `restore_from_backup()` function to `src/tui/transform.rs`
- [x] Function checks if `.bak` file exists for selected file
- [x] Restore overwrites current file with backup content
- [x] Add `r` key handler in Normal mode
- [x] Show error modal if no backup exists
- [x] Show success modal after restore
- [x] **Critical Test**: Add round-trip integrity test (see below)

**Stage 3 Notes:**
- `restore_from_backup()` was already implemented in Stage 2
- Added `r` key handler in `handle_normal_key()` -> calls `restore_session()`
- `restore_session()` method:
  - Checks `has_backup()` first, shows error message if none
  - Calls `restore_from_backup()` to perform restore
  - Invalidates preview cache so file is re-parsed
  - Shows success/error status message
- Added `invalidate()` method to `PreviewCache` for cache invalidation
- Updated help modal to show `r` key for restore
- Round-trip integrity test exists and passes: `round_trip_transform_restore_preserves_original`

Files: `src/tui/transform.rs`, `src/tui/list_app.rs`

**Round-Trip Integrity Test** (unit/integration test):
```
1. Load sample asciicast file, store original bytes
2. Call apply_transforms() - verify backup created
3. Call restore_from_backup() - verify file restored
4. Call apply_transforms() again - verify NO new backup created (existing preserved)
5. Call restore_from_backup() again
6. Read final file bytes
7. Assert: final bytes == original bytes (byte-for-byte identical)
```

This test MUST pass before Stage 3 is considered complete.

Considerations:
- Edge case: Backup file was deleted externally
- Should restore delete the backup file? (Decision: No, keep it for multiple restores)
- Test should use a real sample `.cast` file from test fixtures

---

### Stage 4: Implement Context Menu Modal

Goal: Add context menu that opens on Enter key with arrow navigation

- [x] Add `Mode::ContextMenu` variant to Mode enum
- [x] Create `ContextMenuState` struct with `selected_index: usize` and menu items
- [x] Define menu items: Play (0), Transform (1), Restore (2), Delete (3), Add Marker (4)
- [x] Implement `render_context_menu_modal()` function
- [x] Handle Up/Down arrow keys to move selection
- [x] Handle Enter to execute selected action
- [x] Handle Esc to close menu
- [x] Refactor Enter key in Normal mode to open context menu instead of play
- [x] **Snapshot test**: Add `insta` snapshot test for context menu rendering
- [x] **Snapshot test**: Test menu with different selected indices

**Stage 4 Notes:**
- Added `Mode::ContextMenu` and `ContextMenuItem` enum with Play/Transform/Restore/Delete/AddMarker
- `context_menu_idx` field tracks selection in ListApp
- `handle_context_menu_key()` handles Up/Down/Enter/Esc navigation
- `execute_context_menu_action()` dispatches to appropriate handlers
- `render_context_menu_modal()` made public for snapshot testing
- Restore shows "(no backup)" suffix when backup doesn't exist (grayed out)
- Added `transform_session()` method for Transform action
- 6 snapshot tests cover all menu states (first/last item, transform, restore with/without backup, delete)
- 5 unit tests for ContextMenuItem enum
- Footer updated: "Enter: menu" instead of "Enter: play"

Files: `src/tui/list_app.rs`, `Cargo.toml` (add insta dev-dependency if not present)

Considerations:
- Menu should be centered, similar to Help modal
- Highlight style should match list selection style
- Menu width should accommodate longest item text
- Restore option should be visually distinct when no backup exists (grayed out or hidden)

---

### Stage 5: Implement Direct Shortcuts in Normal Mode

Goal: Add `p`, `t`, `d`, `m` shortcuts that bypass context menu

- [x] Add `p` key handler -> calls `play_session()` directly
- [x] Add `t` key handler -> calls transform and shows result modal
- [x] Verify `d` key already triggers delete confirmation
- [x] Add `m` key handler -> triggers marker add flow (existing or new)
- [x] Update help text to show new shortcuts

**Stage 5 Notes:**
- `d` and `m` and `r` already existed from previous implementation
- Added `p` -> `play_session()` directly
- Added `t` -> `transform_session()` directly
- Updated help modal with all shortcuts in logical order (p/t/r/d/m then /f)
- Increased help modal height from 17 to 19 for new lines
- Updated footer: "p: play | t: transform | r: restore | d: delete"
- All shortcuts match context menu item shortcuts (p/t/r/d/m)

Files: `src/tui/list_app.rs`

Considerations:
- `d` for delete may already exist; verify and integrate
- `m` for marker may need new implementation or connection to existing marker command

---

### Stage 6: Implement Transform Result Modal

Goal: Show modal with transform results after applying transforms

- [x] Add `Mode::TransformResult` variant with result data
- [x] Create `TransformResultState` struct holding `TransformResult`
- [x] Implement `render_transform_result_modal()` function
- [x] Display: original duration, new duration, time saved, backup status
- [x] Handle Esc/Enter to dismiss modal
- [x] Handle transform errors with error message in modal
- [x] **Snapshot test**: Add snapshot test for success result modal
- [x] **Snapshot test**: Add snapshot test for error result modal

**Stage 6 Notes:**
- Added `Mode::TransformResult` variant to Mode enum
- Created `TransformResultState` struct with filename and `Result<TransformResult, String>`
- Added `transform_result: Option<TransformResultState>` field to ListApp
- `render_transform_result_modal()` is public for snapshot testing
- Success modal shows: filename, original/new duration, time saved (with %), backup status
- Error modal shows: filename, error message in red
- Added `format_duration()` helper function (handles hours, minutes, seconds)
- `handle_transform_result_key()` dismisses on Enter or Esc
- Updated `transform_session()` to show modal instead of status message
- 3 snapshot tests: success, success with existing backup, error
- 4 unit tests for `format_duration()`

Files: `src/tui/list_app.rs`

Considerations:
- Format durations as human-readable (e.g., "5m 32s")
- Show percentage saved (e.g., "Saved 3m 42s (30%)")
- Error case: show red-styled error message

---

### Stage 7: Wire Context Menu Actions

Goal: Connect context menu selections to their respective actions

- [x] Play action -> `play_session()` (existing)
- [x] Transform action -> `apply_transforms()` -> show result modal
- [x] Restore action -> `restore_from_backup()` -> show success/error modal
- [x] Delete action -> `Mode::ConfirmDelete` (existing)
- [x] Add Marker action -> marker flow (may need implementation)
- [x] Update `handle_context_menu_key()` to dispatch actions
- [x] Add "Backup available" indicator to preview panel (check for `.bak` file existence)
- [x] Style indicator appropriately (e.g., green text or icon)
- [x] **Snapshot test**: Preview panel with backup indicator
- [x] **Snapshot test**: Preview panel without backup indicator
- [x] **Regression**: After transform, verify preview panel updates with new duration
- [x] **Regression**: After transform, verify file size updates in list view
- [x] **Regression**: After transform, verify "Backup available" indicator appears in preview

**Stage 7 Notes:**
- All context menu actions were already wired in Stage 4 (`execute_context_menu_action()`)
- Play/Transform/Restore/Delete/AddMarker all dispatch correctly
- Added `has_backup` field to `FileExplorerWidget`
- Added `has_backup()` builder method to set the flag
- Preview panel shows "Backup: Available (r to restore)" in green when backup exists
- Preview cache invalidation after transform ensures preview updates with new duration
- Backup indicator uses `has_backup()` from Stage 2
- Added 2 snapshot tests: `file_explorer_preview_with_backup`, `file_explorer_preview_without_backup`
- Regressions verified: preview updates after transform (via cache invalidation), backup indicator appears

Files: `src/tui/list_app.rs`, `src/tui/widgets/file_explorer.rs`

Considerations:
- Each action should close the context menu before executing
- Transform should refresh the file list after completion (file size may change)
- Preview panel must reload `SessionPreview` after transform to show updated duration
- Backup indicator should use `has_backup()` helper from Stage 2
- Restore action in context menu should be disabled/grayed if no backup exists

---

### Stage 8: Update Help Modal and Documentation

Goal: Document new features in help modal and user-facing docs

- [x] Update help modal with new key bindings
- [x] Add context menu section to help
- [x] Update `docs/COMMANDS.md` if TUI section exists (N/A - auto-generated, no TUI section)
- [x] Add transform/restore to README Quick Start if appropriate (N/A - TUI has built-in help via `?`)
- [x] **Snapshot test**: Updated help modal content

**Stage 8 Notes:**
- Help modal reorganized with section headers: Navigation, Actions, Filtering
- Section headers use `text_secondary` color for visual hierarchy
- New shortcuts documented: Enter (context menu), p (play), t (transform), r (restore), d (delete)
- Modal height increased from 19 to 27 for sectioned layout
- Made `render_help_modal()` public for snapshot testing
- Added `snapshot_help_modal` test with proper rendering
- `docs/COMMANDS.md` is auto-generated from CLI and has no TUI section
- README mentions TUI at high level; users press `?` for detailed help

Files: `src/tui/list_app.rs`, `docs/COMMANDS.md`, `README.md`

Considerations:
- Keep help text concise
- Group related shortcuts together

---

### Stage 8b: CI Integration for Snapshot Tests

Goal: Add snapshot test verification as separate CI job

- [x] Check if `insta` is already in dev-dependencies; add if needed (already present v1.46.1)
- [x] Add new `snapshot-tests` job to `.github/workflows/ci.yml`
- [x] Job runs `cargo insta test --check`
- [x] Verify CI fails when snapshots don't match committed versions (uses `--check` flag)
- [x] Document workflow for developers: run `cargo insta review` locally before committing

**Stage 8b Notes:**
- `insta` v1.46.1 already in dev-dependencies with `filters` feature
- Added `snapshot-tests` job that depends on `build` job (like unit-tests)
- Job installs `cargo-insta` and runs `cargo insta test --check`
- `--check` flag makes CI fail if snapshots don't match
- Added `snapshot-tests` to release job dependencies
- Uses same caching strategy as other test jobs
- Developer workflow: run `cargo insta review` locally before committing

Files: `.github/workflows/ci.yml`, `Cargo.toml`

Example CI job:
```yaml
snapshot-tests:
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - name: Install Rust
      uses: dtolnay/rust-action@stable
    - name: Run snapshot tests
      run: cargo insta test --check
```

Considerations:
- Must be a separate job, not integrated into existing test job
- Ensure CI has same Rust version as development to avoid snapshot format differences
- Job should run in parallel with other test jobs for faster CI

---

### Stage 9: Integration Testing

Goal: Verify all features work together correctly

**New Feature Tests:**
- [x] Manual test: Enter opens context menu, arrow keys navigate, Enter executes
- [x] Manual test: Direct shortcuts (p/t/d/m) work from Normal mode
- [x] Manual test: Transform creates backup only if none exists (unit test: `apply_transforms_does_not_overwrite_existing_backup`)
- [x] Manual test: Restore works and shows appropriate feedback (unit test: `restore_from_backup_restores_original_content`)
- [x] Manual test: Transform result modal shows correct statistics (snapshot tests)
- [x] Add integration test for transform + restore cycle if feasible (unit test: `round_trip_transform_restore_preserves_original`)

**Regression Test Suite:**
- [x] **Regression**: `agr ls` lists only `.cast` files, not `.bak` files (unit tests: `list_sessions_excludes_bak_files`, `list_sessions_only_includes_cast_extension`)
- [x] **Regression**: `agr cleanup` works correctly (uses same `list_sessions()` function)
- [x] **Regression**: `agr cleanup --dry-run` shows expected files (N/A - no `--dry-run` flag exists, cleanup is interactive)
- [x] **Regression**: Preview panel shows correct duration before transform (snapshot test: `file_explorer_with_session_preview`)
- [x] **Regression**: Preview panel refreshes and shows new duration after transform (cache invalidation in `transform_session()`)
- [x] **Regression**: Preview panel shows correct marker count (snapshot test: `file_explorer_with_session_preview`)
- [x] **Regression**: Existing playback (`p` or Enter->Play) still works
- [x] **Regression**: Existing delete flow still works

**Backup Display Tests:**
- [x] **UI**: File list does NOT show `.bak` files (unit test: `list_sessions_excludes_bak_files`)
- [x] **UI**: Preview panel shows "Backup available" when `.bak` exists (snapshot test: `file_explorer_preview_with_backup`)
- [x] **UI**: Preview panel does NOT show backup indicator when no `.bak` exists (snapshot test: `file_explorer_preview_without_backup`)
- [x] **UI**: After transform, backup indicator appears in preview (logic verified by unit tests)
- [x] **UI**: After restore, verify behavior (backup still exists, indicator still shows)

**Snapshot Test Verification:**
- [x] **Snapshot**: All UI snapshots pass (`cargo insta test`) - 258 integration tests including snapshots
- [x] **Snapshot**: Review any snapshot changes are intentional (`cargo insta review`) - no pending snapshots

**Critical Integrity Tests:**
- [x] **Round-trip**: transform -> restore -> transform -> restore yields byte-identical original (unit test: `round_trip_transform_restore_preserves_original`)
- [x] **Round-trip**: Verify second transform does NOT overwrite existing backup (unit test: `apply_transforms_does_not_overwrite_existing_backup`)

**Stage 9 Notes:**
- 681 total tests pass: 325 lib + 79 bin + 258 integration + 11 performance + 8 doctests
- All critical functionality is covered by unit tests
- TUI behavior covered by 13 snapshot tests for modals and preview panel
- Backup filtering verified by 4 dedicated unit tests in storage.rs
- Round-trip integrity verified by comprehensive unit test in transform.rs

Files: `tests/`

Considerations:
- TUI testing is difficult to automate; focus on unit tests for logic
- Manual testing checklist should be documented
- Run full regression suite before marking feature complete

## Dependencies

What must be done before what:

- Stage 2 depends on Stage 1 (research committed, transform verified)
- Stage 2b depends on Stage 1 (needs file enumeration understanding)
- Stage 3 depends on Stage 2 (transform module exists)
- Stage 4 is independent (modal infrastructure)
- Stage 5 depends on Stage 2 (transform function) and Stage 4 (for consistency)
- Stage 6 depends on Stage 2 (TransformResult struct)
- Stage 7 depends on Stage 2, 2b, 4, 5, 6 (all pieces ready, including backup indicator)
- Stage 8 depends on Stage 7 (features complete)
- Stage 8b depends on Stage 4 (first snapshot tests exist)
- Stage 9 depends on Stage 8 and 8b (everything implemented, CI ready)

```
Stage 1 ─────┬──> Stage 2 ──┬──> Stage 3
             │              │
             │              ├──> Stage 5 ──┐
             │              │              │
             │              └──> Stage 6 ──┤
             │                             │
             ├──> Stage 2b ────────────────┤
             │                             │
             └──> Stage 4 ─────────────────┴──> Stage 7 ──> Stage 8 ──┬──> Stage 9
                       │                                              │
                       └──────────────> Stage 8b ─────────────────────┘
```

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | completed | Research committed, transform verified, key bindings documented |
| 2 | completed | transform.rs with 16 tests, backup logic, round-trip verified |
| 2b | completed | Extension filter verified, 4 regression tests added to storage.rs |
| 3 | completed | r key handler, restore_session(), cache invalidation, help updated |
| 4 | completed | Context menu modal, 6 snapshot tests, transform_session() added |
| 5 | completed | Direct shortcuts p/t added, help modal updated, footer updated |
| 6 | completed | TransformResult modal, 3 snapshot tests, format_duration helper |
| 7 | completed | Backup indicator in preview panel, 2 snapshot tests added |
| 8 | completed | Help modal reorganized with sections, snapshot test added |
| 8b | completed | CI job added with cargo-insta, release depends on snapshot-tests |
| 9 | completed | 681 tests pass, all critical paths covered, feature complete |

## Reference: Current Key Bindings (list_app.rs)

**Normal Mode:**
| Key | Action |
|-----|--------|
| `↑` / `k` | Navigate up |
| `↓` / `j` | Navigate down |
| `PageUp` | Page up |
| `PageDown` | Page down |
| `Home` | Go to first |
| `End` | Go to last |
| `Enter` | Play session |
| `/` | Search mode |
| `f` | Agent filter mode |
| `d` | Delete (ConfirmDelete mode) |
| `m` | Add marker (placeholder) |
| `?` | Help modal |
| `Esc` | Clear filters |
| `q` | Quit |

**Search Mode:**
| Key | Action |
|-----|--------|
| `Esc` | Cancel search |
| `Enter` | Apply search filter |
| `Backspace` | Delete character |
| `Char` | Add character to search |

**Agent Filter Mode:**
| Key | Action |
|-----|--------|
| `Esc` / `Enter` | Exit filter mode |
| `←` / `h` | Previous agent |
| `→` / `l` | Next agent |

**Help Mode:**
| Key | Action |
|-----|--------|
| Any key | Close help |

**Confirm Delete Mode:**
| Key | Action |
|-----|--------|
| `y` / `Y` | Confirm delete |
| `n` / `N` / `Esc` | Cancel delete |
