# Plan: Add Rename Action to `agr ls` TUI

References: ADR.md

## Open Questions

All resolved:

1. ~~Cursor position on pre-fill~~ → **Select entire stem** (first keystroke replaces all)
2. ~~Invalid characters~~ → **Reject invalid chars as typed** using `INVALID_CHARS` from `filename.rs`
3. ~~Maximum name length~~ → **Enforce 255 char limit during typing** via `filename::MAX_FILENAME_LENGTH`

---

## Stages

### Stage 1: Remove AddMarker and Clean Up Context Menu

**Goal**: Remove the placeholder AddMarker action, remove the silence hint line, and reorder the context menu. This is a standalone cleanup that reduces the menu from 7 items (with AddMarker) to 6 items (before Rename is added in Stage 3).

#### Changes

**`src/tui/list_app.rs` -- ContextMenuItem enum:**
- [ ] Remove `ContextMenuItem::AddMarker` variant
- [ ] Update `ContextMenuItem::ALL` array: remove AddMarker, reorder to [Play, Copy, Optimize, Analyze, Restore, Delete] (6 items)
- [ ] Remove `AddMarker` arms from `label()` and `shortcut()` matches
- [ ] Remove `Restore` shortcut: change `shortcut()` to return `""` for Restore
- [ ] Add `has_shortcut()` method: returns `!self.shortcut().is_empty()` — used by rendering

**`src/tui/list_app.rs` -- Key handlers:**
- [ ] Remove `KeyCode::Char('m')` arm from `handle_normal_key()`
- [ ] Remove `KeyCode::Char('m')` arm from `handle_context_menu_key()`
- [ ] Remove `KeyCode::Char('r')` arm from `handle_context_menu_key()` (Restore shortcut removed; will be re-added for Rename in Stage 3)

**`src/tui/list_app.rs` -- `execute_context_menu_action()`:**
- [ ] Remove `ContextMenuItem::AddMarker => self.add_marker()?` arm

**`src/tui/list_app.rs` -- `add_marker()` method:**
- [ ] Delete the `add_marker()` method entirely

**`src/tui/list_app.rs` -- `render_context_menu_modal()`:**
- [ ] Remove the silence hint block (the `if matches!(item, ContextMenuItem::Optimize)` that adds "Removes silence from recording")
- [ ] Update `modal_height` calculation (remove the `+ optimize hint` allowance)
- [ ] Fix shortcut rendering: use conditional format — show `"  {} ({})"` only when `has_shortcut()`, otherwise `"  {}"` (no empty parens for Restore)

**`src/tui/list_app.rs` -- `render_help_modal()`:**
- [ ] No changes in this stage (help modal does not mention `m` or `r` for Restore; it stays as-is)

**`src/tui/list_app.rs` -- Footer:**
- [ ] No changes in this stage (footer does not mention `m` or `r`)

**`src/tui/list_app.rs` -- Tests:**
- [ ] Update `context_menu_has_seven_items` test: rename to `context_menu_has_six_items`, assert `ALL.len() == 6`
- [ ] Update `context_menu_item_order` test: assert new order [Play, Copy, Optimize, Analyze, Restore, Delete]
- [ ] Remove or update `context_menu_copy_label_and_shortcut` if it references AddMarker
- [ ] Fix `context_menu_items_have_shortcuts` test: change assertion to allow empty shortcut for menu-only items (Restore), e.g. check `has_shortcut()` returns `true` for items with shortcuts and `false` for Restore

**Snapshot tests to regenerate:**
- [ ] `context_menu_first_item.snap`
- [ ] `context_menu_last_item.snap`
- [ ] `context_menu_delete_selected.snap`
- [ ] `context_menu_restore_no_backup.snap`
- [ ] `context_menu_restore_with_backup.snap`
- [ ] `context_menu_transform_selected.snap`

**Verify:** `cargo test tui::list_app && cargo test --test snapshot_tui_test`

---

### Stage 2: Add Input Validation Helpers + Mode::RenameInput + Key Handling

**Goal**: Expose validation helpers from `filename.rs` that the TUI needs, then add `Mode::RenameInput`, key handling, and context menu integration. No filesystem rename logic yet.

#### Changes

**`src/files/filename.rs` -- Public helpers (needed by TUI for real-time input validation):**
- [ ] Make `INVALID_CHARS` public: `pub const INVALID_CHARS`
- [ ] Make `MAX_FILENAME_LENGTH` public: `pub const MAX_FILENAME_LENGTH`
- [ ] Add `pub fn is_valid_filename_char(c: char) -> bool` — returns `!INVALID_CHARS.contains(&c)`
- [ ] Remove `#[allow(dead_code)]` from `validate_length()` (now used)

**`src/tui/list_app.rs` -- Mode enum:**
- [ ] Add `RenameInput` variant to `Mode` enum
- [ ] Add arm to `Mode::to_shared()`: `Mode::RenameInput => None` (app-specific, not shared)

**`src/tui/list_app.rs` -- ListApp struct:**
- [ ] Add `rename_input: String` field
- [ ] Add `rename_selected_all: bool` field (tracks if entire stem is still "selected")
- [ ] Initialize as `String::new()` and `false` in `ListApp::new()`

**`src/tui/list_app.rs` -- `handle_normal_key()`:**
- [ ] Add `KeyCode::Char('r')` arm:
  - Check `is_selected_locked()` -> `Mode::ConfirmUnlock`
  - Check `selected_item().is_some()`
  - Pre-fill `rename_input` with current filename stem (strip `.cast` extension)
  - Set `rename_selected_all = true`
  - Set `mode = Mode::RenameInput`

**`src/tui/list_app.rs` -- New `handle_rename_input_key()` method:**
- [ ] `KeyCode::Esc` -> set `mode = Mode::Normal`
- [ ] `KeyCode::Enter` -> call `self.rename_session()` (implemented in Stage 3), set `mode = Mode::Normal`
- [ ] `KeyCode::Backspace`:
  - If `rename_selected_all` -> clear entire `rename_input`, set `rename_selected_all = false`
  - Else -> pop last char from `rename_input`
- [ ] `KeyCode::Char(c)`:
  - Reject if `!filename::is_valid_filename_char(c)` (uses `INVALID_CHARS`)
  - Reject if total length would exceed `MAX_FILENAME_LENGTH - ext_len` (compute dynamically from selected item's extension, e.g. `.cast` = 5)
  - If `rename_selected_all` -> replace entire `rename_input` with `c`, set `rename_selected_all = false`
  - Else -> push `c` to `rename_input`
- [ ] All other keys -> no-op (consumed)

**`src/tui/list_app.rs` -- `handle_key()` (TuiApp impl):**
- [ ] Add `Mode::RenameInput => self.handle_rename_input_key(key)?` arm in app-specific match

**`src/tui/list_app.rs` -- `draw()` (TuiApp impl):**
- [ ] Add `Mode::RenameInput` arm in status_text match:
  - If `rename_selected_all`: show `"Rename: [current_stem]"` (brackets indicate selection)
  - Else: show `"Rename: {rename_input}_"` (underscore cursor at end)
- [ ] Add `Mode::RenameInput` arm in footer_text match: `"Enter: confirm | Esc: cancel | Backspace: delete char"`

**`src/tui/list_app.rs` -- Context menu integration:**
- [ ] Add `ContextMenuItem::Rename` variant to enum
- [ ] Update `ContextMenuItem::ALL` array: insert Rename at position 2 (after Copy), making it 7 items: [Play, Copy, Rename, Optimize, Analyze, Restore, Delete]
- [ ] Add `label()` arm: `"Rename"`
- [ ] Add `shortcut()` arm: `"r"`
- [ ] Add `KeyCode::Char('r')` arm to `handle_context_menu_key()` (points to Rename)
- [ ] Add `ContextMenuItem::Rename` arm to `execute_context_menu_action()`: pre-fill input, set `mode = Mode::RenameInput`

**`src/tui/list_app.rs` -- Help modal:**
- [ ] Add `r` / `Rename` line in Actions section (after Copy, before Optimize)
- [ ] Update `modal_height` if needed

**`src/tui/list_app.rs` -- Footer:**
- [ ] Add `r: rename` to Normal mode footer text

**Tests:**
- [ ] `mode_equality` covers RenameInput
- [ ] `context_menu_has_six_items` -> rename to `context_menu_has_seven_items`, assert `ALL.len() == 7`
- [ ] `context_menu_item_order` updated for new 7-item order
- [ ] New test: `rename_mode_esc_returns_to_normal`
- [ ] New test: `rename_mode_backspace_removes_char`
- [ ] New test: `rename_mode_char_appends`
- [ ] New test: `rename_mode_rejects_invalid_chars` (uses `INVALID_CHARS`)
- [ ] New test: `rename_mode_enforces_length_limit`
- [ ] New test: `rename_selected_all_first_char_replaces`
- [ ] New test: `rename_selected_all_backspace_clears`
- [ ] New test: `rename_prefills_current_stem`

**Verify:** `cargo test tui::list_app && cargo test --test snapshot_tui_test`

---

### Stage 3: Implement Rename Filesystem Logic

**Goal**: Add a generalized `rename_file()` function to `src/files/filename.rs` that handles validation and filesystem rename. Then wire it into `list_app.rs` via a thin `rename_session()` method. Fix `update_item_path()` to re-sort/re-filter.

#### Changes

**`src/files/filename.rs` -- New `rename_file()` public function:**
- [ ] Signature: `pub fn rename_file(old_path: &Path, new_stem: &str) -> Result<PathBuf, RenameError>`
- [ ] Generalized — preserves original extension, works for any file type
- [ ] Validate `new_stem`:
  - Empty -> `RenameError::EmptyName`
  - Same as current stem -> return `Ok(old_path.to_path_buf())` (no-op)
  - Contains any char from `INVALID_CHARS` -> `RenameError::InvalidChars`
  - Is a Windows reserved name (reuse `handle_reserved_name()` logic) -> `RenameError::ReservedName`
  - New filename (stem + extension) exceeds `MAX_FILENAME_LENGTH` -> `RenameError::TooLong`
- [ ] Build new path: same parent directory + `new_stem` + original extension
- [ ] Check if new path already exists -> `RenameError::AlreadyExists(new_filename)`
- [ ] Call `std::fs::rename(old_path, new_path)`
- [ ] If backup exists (`backup::backup_path_for(old_path)`), rename backup too (best-effort, ignore errors)
- [ ] Return `Ok(new_path)`

**`src/files/filename.rs` -- New `RenameError` enum:**
- [ ] `EmptyName`, `InvalidChars`, `ReservedName`, `TooLong`, `AlreadyExists(String)`, `IoError(std::io::Error)`

**`src/tui/list_app.rs` -- New thin `rename_session()` method:**
- [ ] Guard: `if let Some(item) = self.shared.explorer.selected_item()` (consistent with all other action methods)
- [ ] Call `filename::rename_file(path, &self.rename_input)`
- [ ] On success:
  - Invalidate preview cache for old path
  - Update explorer via `update_item_path()`
  - Re-apply sort and filter (name change affects sort-by-name and search filter)
  - Set status message: "Renamed to {new_name}"
- [ ] On error: map `RenameError` variants to user-friendly status messages

**`src/tui/widgets/file_explorer.rs` -- Fix `update_item_path()`:**
- [ ] After updating item fields, call `apply_filter()` and `apply_sort()` to maintain correct ordering and filter state after name change

**Tests in `src/files/filename.rs`:**
- [ ] `rename_file_empty_name_errors`
- [ ] `rename_file_same_name_is_noop`
- [ ] `rename_file_success` (tempfile — verifies extension preserved)
- [ ] `rename_file_renames_backup_too` (tempfile + .bak)
- [ ] `rename_file_conflict_errors` (existing target)
- [ ] `rename_file_invalid_chars_errors`
- [ ] `rename_file_preserves_extension` (test with .cast, .txt, etc.)
- [ ] `rename_file_reserved_name_errors` (CON, NUL, etc.)
- [ ] `is_valid_filename_char_rejects_invalid`

**Verify:** `cargo test files::filename && cargo test tui::list_app`

---

### Stage 4: Update Snapshot Tests

**Goal**: Regenerate all affected snapshot tests and verify the final visual output.

#### Changes

- [ ] Run `cargo test --test snapshot_tui_test -- --force-update-snapshots` (or equivalent insta flag `INSTA_UPDATE=1`)
- [ ] Review each updated snapshot for correctness:
  - `context_menu_first_item.snap` -- Play selected, 7 items in new order, no silence hint
  - `context_menu_last_item.snap` -- Delete selected (last item)
  - `context_menu_delete_selected.snap` -- Delete highlighted
  - `context_menu_restore_no_backup.snap` -- Restore with "no backup" suffix, no shortcut
  - `context_menu_restore_with_backup.snap` -- Restore without shortcut
  - `context_menu_transform_selected.snap` -- Optimize highlighted, no silence hint
  - `help_modal.snap` -- includes `r Rename` line, no `m Add marker` line
- [ ] Commit snapshot updates

**Verify:** `cargo test --test snapshot_tui_test`

---

### Stage 5: Documentation Update

**Goal**: Update module-level doc comments and the doc comment on the file.

#### Changes

- [ ] Update module doc comment at top of `src/tui/list_app.rs`: remove "add marker" from feature list, add "rename"
- [ ] Review `README.md` TUI controls section: add `r` Rename, remove `m` Add marker if mentioned
- [ ] Run `cargo xtask gen-docs` if applicable

**Verify:** `cargo doc --no-deps`

---

## Dependencies

```
Stage 1 (Cleanup) ──> Stage 2 (Helpers + Mode + Keys) ──> Stage 3 (Filesystem Logic)
                                                                  │
                                                                  v
                                                          Stage 4 (Snapshots)
                                                                  │
                                                                  v
                                                          Stage 5 (Docs)
```

All stages are sequential. Stage 1 changes `ContextMenuItem::ALL` and item count. Stage 2 exposes validation helpers from `filename.rs` AND adds TUI mode/keys (both needed before Stage 3). Stage 3 adds `rename_file()` and wires it into `rename_session()`. Stages 4-5 finalize.

---

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | pending | Remove AddMarker, silence hint, reorder menu |
| 2 | pending | Validation helpers + Mode::RenameInput + key handling + menu item |
| 3 | pending | Filesystem rename logic |
| 4 | pending | Snapshot test regeneration |
| 5 | pending | Documentation |

---

## Test Commands

```bash
# Run list_app unit tests
cargo test tui::list_app

# Run snapshot TUI tests
cargo test --test snapshot_tui_test

# Run all TUI tests
cargo test tui

# Update snapshots (review diffs carefully)
INSTA_UPDATE=1 cargo test --test snapshot_tui_test

# Full test suite
cargo test

# Check for warnings/lints
cargo clippy -- -D warnings
```
