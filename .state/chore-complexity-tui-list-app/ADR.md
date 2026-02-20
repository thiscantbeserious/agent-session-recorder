# Sub-ADR: tui/list_app -- 5 Violations

Parent: [ADR.md](ADR.md)

## Scope

File: `src/tui/list_app.rs` (1675 lines)
Violations: 5 functions, combined score 181

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `handle_input()` | 1426 | `ListApp::draw()` (via `TuiApp` impl) | 1426 | 80 |
| `handle_key()` | 1254 | `ListApp::handle_mouse()` (via `TuiApp` impl) | 1254 | 48 |
| `handle_scroll()` | 704 | `ListApp::handle_rename_input_key()` | 704 | 18 |
| `process_event()` | 1047 | `ListApp::render_context_menu_modal()` | 1047 | 18 |
| `new()` | 213 | `ListApp::handle_normal_key()` | 213 | 17 |

## Function Analysis

### 1. `draw()` at line 1426 -- score 80

**Signature:**
```rust
fn draw(&mut self) -> Result<()>   // impl TuiApp for ListApp
```

**Current structure:**
Lines 1426-1652. The function:
1. Calculates terminal size and sets page size (lines 1427-1431)
2. Polls preview cache and prefetches adjacent previews (lines 1433-1435)
3. Extracts shared fields into local variables before the closure (lines 1437-1449)
4. Builds preview and backup state (lines 1452-1467)
5. Calls `self.app.draw(|frame| { ... })` with a large closure (lines 1469-1649)

Inside the closure (lines 1469-1649):
- Renders file explorer list (lines 1472-1480) -- conditional on `mode == Mode::RenameInput`
- **Status line match block** (lines 1484-1579) -- 11-arm match on `mode`, biggest complexity contributor. The `Mode::RenameInput` arm (lines 1498-1521) has deeply nested `if rename_selected_all` / `if !before.is_empty()` / `if !after.is_empty()` conditions building spans.
- **Footer text match block** (lines 1582-1607) -- 12-arm match returning static `&str`. The `Mode::Import` arm (lines 1593-1603) has a nested match on `state.phase`.
- **Modal overlay match block** (lines 1610-1648) -- 8-arm match delegating to render functions. Several arms have nested `if let Some(item) = explorer.selected_item()`.

**Why complexity is high:** Three large `match mode` blocks stacked sequentially inside a closure. Each has many arms, and several arms contain further nesting (conditionals, `if let`).

**Borrow checker constraint:** `self.app.draw(|frame| {...})` takes `&mut self.app`, preventing the closure from borrowing any other field through `self`. All `self` fields are pre-extracted into local variables before the closure (lines 1437-1449). Extracted helpers MUST be **free functions** (not `&self` methods), taking these locals as explicit parameters.

**Extraction targets:**

1. `render_status_line_for_mode()` -- free function. Takes `(frame, area, mode, search_input, available_agents, agent_filter_idx, rename_input, rename_cursor, rename_selected_all, import_state, status, explorer, selected_name)`. Replaces lines 1484-1579. Returns nothing (renders directly to frame). The inner `Mode::RenameInput` arm can further delegate to a `build_rename_status_spans()` helper returning `Vec<Span>`.

2. `footer_text_for_mode()` -- free function. Takes `(mode, import_state) -> &str`. Replaces lines 1582-1607. Each arm is a static string except `Mode::Import` which matches on `state.phase`.

3. `render_modal_overlays()` -- free function. Takes `(frame, area, mode, explorer, context_menu_idx, backup_exists, optimize_result, import_state)`. Replaces lines 1610-1648. Each arm delegates to an existing `render_*` function.

### 2. `handle_mouse()` at line 1254 -- score 48

**Signature:**
```rust
fn handle_mouse(&mut self, mouse: MouseEvent) -> Result<()>   // impl TuiApp for ListApp
```

**Current structure:**
Lines 1254-1378. Top-level `match self.mode` with 6 arms:
- `Mode::Normal` (lines 1258-1285): nested `match mouse.kind` with 3 arms. The `Down(Left)` arm (lines 1266-1283) has nested click-to-index calculation with `if click_row >= 1 && ...` and locked-item redirect.
- `Mode::ContextMenu` (lines 1287-1328): nested `match mouse.kind` with 3 arms. The `Down(Left)` arm (lines 1300-1326) has modal geometry calculation and hit testing.
- `Mode::ConfirmDelete | Mode::ConfirmUnlock` (lines 1330-1347): delegates to `modals::handle_confirm_click` with mode-conditional width and result mapping.
- `Mode::ConfirmDeleteFinal | Mode::ConfirmUnlockFinal` (lines 1349-1367): nearly identical to above.
- `_` wildcard (lines 1369-1375): click-outside-dismisses for remaining modal modes.

**Why complexity is high:** Doubly nested `match` (mode then mouse.kind), with further nesting inside click handlers for geometry and hit testing.

**Borrow checker constraint:** This is `&mut self`, so extracted helpers can be methods on `ListApp`. No conflicting borrows.

**Extraction targets:**

1. `handle_normal_mouse(&mut self, mouse: MouseEvent, explorer_height: u16)` -- method. Covers lines 1258-1285.
2. `handle_context_menu_mouse(&mut self, mouse: MouseEvent, width: u16, height: u16)` -- method. Covers lines 1287-1328.
3. `handle_confirm_modal_mouse(&mut self, mouse: MouseEvent, width: u16, height: u16)` -- method. Merges the two confirm-modal arms (lines 1330-1367) into one parameterized by the current mode.

### 3. `handle_rename_input_key()` at line 704 -- score 18

**Signature:**
```rust
fn handle_rename_input_key(&mut self, key: KeyEvent) -> Result<()>
```

**Current structure:**
Lines 704-799. A `match key.code` with 9 arms: `Esc`, `Enter`, `Backspace`, `Delete`, `Left`, `Right`, `Home`, `End`, `Char(c)`, and `_`.

The complexity sources are:
- `Backspace` arm (lines 716-730): `if self.rename_selected_all { ... } else if self.rename_cursor > 0 { ... }` with char boundary navigation.
- `Delete` arm (lines 732-741): `if self.rename_selected_all { ... } else if self.rename_cursor < ... { ... }`.
- `Char(c)` arm (lines 770-796): validates char, computes max stem length via chained `map`/`unwrap_or`, then `if self.rename_selected_all { ... } else if ... < max_stem_len { ... }`.

**Borrow checker constraint:** `&mut self`, no conflicting borrows. Extracted helpers can be methods.

**Extraction targets:**

1. `handle_rename_backspace(&mut self)` -- method. Covers lines 716-730. Handles select-all clear vs. char-boundary deletion.
2. `handle_rename_delete(&mut self)` -- method. Covers lines 732-741. Handles select-all clear vs. forward deletion.
3. `handle_rename_char_input(&mut self, c: char)` -- method. Covers lines 770-796. Handles validation, max-length check, select-all replacement, and insert-at-cursor.

### 4. `render_context_menu_modal()` at line 1047 -- score 18

**Signature:**
```rust
pub fn render_context_menu_modal(frame: &mut Frame, area: Rect, selected_idx: usize, backup_exists: bool)
```

**Current structure:**
Lines 1047-1130. An associated function (already `Self::`, not `&self`). Iterates over `ContextMenuItem::ALL` (lines 1077-1109) building styled `Line` spans. Complexity from:
- `is_restore && !backup_exists` check (line 1083) with nested `if item.has_shortcut()` (lines 1083-1093) for label formatting.
- `if is_selected / else if is_disabled / else` style chain (lines 1095-1101).
- Selection prefix logic (line 1104).

**Borrow checker constraint:** This is already a static method (`pub fn`, not `&self`). No borrow issues.

**Extraction targets:**

1. `build_menu_item_label(item: &ContextMenuItem, backup_exists: bool) -> String` -- free function. Takes the item and backup flag, returns the formatted label with shortcut and disabled hints. Covers lines 1083-1093.
2. `menu_item_style(is_selected: bool, is_disabled: bool, theme: &Theme) -> Style` -- free function. Returns the appropriate style. Covers lines 1095-1101.

### 5. `handle_normal_key()` at line 213 -- score 17

**Signature:**
```rust
fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()>
```

**Current structure:**
Lines 213-279. A `match key.code` with 9 arms. The complexity comes from 6 arms (`Enter`, `Char('p')`, `Char('t')`, `Char('a')`, `Char('r')`, `Char('d')`) all containing the identical pattern:
```rust
if self.is_selected_locked() {
    self.mode = Mode::ConfirmUnlock;
    return Ok(());
}
```
This repeated pattern at lines 217-219, 229-231, 237-239, 243-245, 250-252, 257-259 adds `+2` complexity for each `if` (nesting increment + condition).

**Borrow checker constraint:** `&mut self`, no issues. Simple method extraction.

**Extraction targets:**

1. `redirect_if_locked(&mut self) -> bool` -- method. Returns `true` (and sets mode to `ConfirmUnlock`) if the selected item is locked, `false` otherwise. Replaces 6 identical code blocks.

## Dependencies Between Functions

- `draw()` and `handle_mouse()` and `handle_normal_key()` all reference `self.mode` and `self.context_menu_idx`. They do not call each other.
- `handle_rename_input_key()` is called from `handle_key()` (line 1419) which is the `TuiApp::handle_key` implementation.
- `render_context_menu_modal()` is called from within `draw()`'s closure (line 1624) and from snapshot tests.
- All 5 functions are in the same file and must be refactored sequentially (bottom-to-top to minimize line-number drift).

## Testability Assessment

**Existing tests in `src/tui/list_app.rs` (lines 1676-1730+):**
- `mode_default_is_normal`, `mode_equality`, `mode_clone_and_copy`, `mode_debug_format` -- test `Mode` enum, not the flagged functions.
- `context_menu_has_seven_items`, `context_menu_items_have_labels` -- test `ContextMenuItem`, relevant to `render_context_menu_modal()`.

**Existing integration/snapshot tests:** `tests/integration/snapshot_tui_test.rs` provides snapshot-based regression tests for the TUI rendering pipeline. These are the primary safety net for `draw()` and `render_context_menu_modal()`.

**TDD approach:**
- `draw()`, `handle_mouse()`, `handle_rename_input_key()`, `handle_normal_key()`: Not directly unit-testable (require terminal). Full `cargo test` is the baseline. Snapshot tests are the regression net.
- `render_context_menu_modal()`: Existing snapshot tests cover this. The extracted `build_menu_item_label()` and `menu_item_style()` functions could have unit tests added, but this is out of scope (pure refactoring).
- `redirect_if_locked()`: Could be unit-tested if `ListApp` can be constructed in test, but this is also out of scope.

## Edit Strategy

Work bottom-to-top in the file to minimize line-number shifts:
1. `draw()` at line 1426 (extract 3 free functions, add before line 1426 or after `impl TuiApp`)
2. `handle_mouse()` at line 1254 (extract 3 methods into `impl ListApp`)
3. `render_context_menu_modal()` at line 1047 (extract 2 free functions)
4. `handle_rename_input_key()` at line 704 (extract 3 methods into `impl ListApp`)
5. `handle_normal_key()` at line 213 (extract 1 method into `impl ListApp`)
