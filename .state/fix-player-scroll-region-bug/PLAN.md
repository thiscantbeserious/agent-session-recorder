# Execution Plan: Fix Player Scroll Region Bug

## Overview

This plan breaks the ADR into small, independently testable stages. Each stage should:
- Compile successfully
- Pass all existing tests
- Be reviewable in isolation

**Total stages:** 14
**Estimated complexity:** Medium (refactor + feature)

## Progress

- [x] Stage 1: Create terminal directory structure
- [x] Stage 2: Extract types.rs
- [x] Stage 3: Move TerminalBuffer to mod.rs
- [x] Stage 4: Move TerminalPerformer to performer.rs
- [x] Stage 5: Extract cursor handlers
- [x] Stage 6: Extract editing handlers
- [x] Stage 7: Extract style handler
- [x] Stage 8: Add observability for unhandled sequences
- [x] Stage 9: Add scroll region state
- [x] Stage 10: Implement scroll_up and scroll_down helpers
- [x] Stage 11: Implement CSI r handler (DECSTBM)
- [x] Stage 12: Implement CSI S and CSI T handlers
- [x] Stage 13: Update line_feed and reverse_index
- [x] Stage 14: Migrate tests to separate files
- [x] Stage 15: Create scroll region test fixture

---

## Phase 1: Module Structure Setup

### Stage 1: Create terminal directory structure
**Goal:** Set up the new top-level module structure without moving any code yet.

**Tasks:**
1. Create `src/terminal/` directory (top-level, not under player/)
2. Create empty placeholder files:
   - `src/terminal/mod.rs`
   - `src/terminal/types.rs`
   - `src/terminal/performer.rs`
   - `src/terminal/handlers/mod.rs`
   - `src/terminal/handlers/cursor.rs`
   - `src/terminal/handlers/scroll.rs`
   - `src/terminal/handlers/editing.rs`
   - `src/terminal/handlers/style.rs`
   - `src/terminal/tests/mod.rs`
3. Add `pub mod terminal;` to `src/lib.rs`
4. Keep old `src/player/terminal.rs` temporarily

**Verification:**
- `cargo check` passes
- All existing tests pass

**Files changed:** 10 new files, 1 modified

---

### Stage 2: Extract types.rs
**Goal:** Move data types to their own file.

**Tasks:**
1. Move to `src/terminal/types.rs`:
   - `Color` enum
   - `CellStyle` struct
   - `Cell` struct
   - `StyledLine` struct
2. Add re-exports in `src/terminal/mod.rs`
3. Update imports in the old `src/player/terminal.rs`

**Verification:**
- `cargo check` passes
- `cargo test terminal` passes

**Files changed:** 2 modified, 1 populated

---

### Stage 3: Move TerminalBuffer to mod.rs
**Goal:** Move the main public struct to the module root.

**Tasks:**
1. Move `TerminalBuffer` struct and its impl block to `src/terminal/mod.rs`
2. Move `impl fmt::Display for TerminalBuffer` to `src/terminal/mod.rs`
3. Add necessary imports (vte::Parser, types)
4. Keep `TerminalPerformer` in old location for now

**Verification:**
- `cargo check` passes
- `cargo test terminal` passes

**Files changed:** 2 modified

---

### Stage 4: Move TerminalPerformer to performer.rs
**Goal:** Extract the performer struct to its own file.

**Tasks:**
1. Move `TerminalPerformer` struct definition to `src/terminal/performer.rs`
2. Move `impl Perform for TerminalPerformer` to `src/terminal/performer.rs`
3. Move all helper methods (line_feed, carriage_return, etc.) to `src/terminal/performer.rs`
4. Update visibility: `pub(crate)` for struct, `pub(super)` for internals
5. Update imports in `mod.rs` to use performer
6. Update `src/player/mod.rs` to re-export from `crate::terminal` instead of local
7. Update `src/player/native.rs` imports to use `crate::terminal`
8. Delete old `src/player/terminal.rs`

**Verification:**
- `cargo check` passes
- `cargo test terminal` passes
- `cargo test player` passes

**Files changed:** 4 modified, 1 populated, 1 deleted

---

## Phase 2: Handler Extraction

### Stage 5: Extract cursor handlers
**Goal:** Move cursor-related handlers to `handlers/cursor.rs`.

**Tasks:**
1. Create handler methods in `cursor.rs`:
   - `handle_cursor_up(&mut self, n: usize)`
   - `handle_cursor_down(&mut self, n: usize)`
   - `handle_cursor_forward(&mut self, n: usize)`
   - `handle_cursor_back(&mut self, n: usize)`
   - `handle_cursor_position(&mut self, row: usize, col: usize)`
   - `handle_cursor_horizontal_absolute(&mut self, col: usize)`
   - `handle_cursor_vertical_absolute(&mut self, row: usize)`
   - `handle_save_cursor(&mut self)`
   - `handle_restore_cursor(&mut self)`
   - `handle_dec_save_cursor(&mut self)` (ESC 7)
   - `handle_dec_restore_cursor(&mut self)` (ESC 8)
2. Update `csi_dispatch` and `esc_dispatch` to call handlers
3. Keep inline code as fallback during transition

**Verification:**
- `cargo check` passes
- `cargo test terminal::tests::cursor` passes
- `cargo test terminal` passes

**Files changed:** 2 modified, 1 populated

---

### Stage 6: Extract editing handlers
**Goal:** Move erase/delete handlers to `handlers/editing.rs`.

**Tasks:**
1. Create handler methods in `editing.rs`:
   - `handle_erase_display(&mut self, mode: u16)`
   - `handle_erase_line(&mut self, mode: u16)`
   - `handle_delete_lines(&mut self, n: usize)`
   - `handle_insert_lines(&mut self, n: usize)`
   - `handle_delete_chars(&mut self, n: usize)`
   - `handle_insert_chars(&mut self, n: usize)`
   - `handle_erase_chars(&mut self, n: usize)`
2. Move helper methods (erase_to_eol, erase_line, etc.) to editing.rs
3. Update `csi_dispatch` to call handlers

**Verification:**
- `cargo check` passes
- `cargo test terminal::tests::editing` passes
- `cargo test terminal` passes

**Files changed:** 2 modified, 1 populated

---

### Stage 7: Extract style handler
**Goal:** Move SGR handler to `handlers/style.rs`.

**Tasks:**
1. Move `handle_sgr(&mut self, params: &[u16])` to `style.rs`
2. Update `csi_dispatch` 'm' case to call handler

**Verification:**
- `cargo check` passes
- `cargo test terminal::tests::style` passes
- `cargo test terminal` passes

**Files changed:** 2 modified, 1 populated

---

### Stage 8: Add observability for unhandled sequences
**Goal:** Log unhandled escape sequences instead of silently ignoring.

**Tasks:**
1. Add `tracing` dependency if not present
2. Create in `handlers/mod.rs`:
   - `log_unhandled_csi(action: char, params: &[u16], intermediates: &[u8])`
   - `log_unhandled_esc(byte: u8, intermediates: &[u8])`
3. Replace `_ => {}` in `csi_dispatch` with call to `log_unhandled_csi`
4. Replace `_ => {}` in `esc_dispatch` with call to `log_unhandled_esc`

**Verification:**
- `cargo check` passes
- `cargo test terminal` passes
- Run with `RUST_LOG=debug` to verify logging works

**Files changed:** 2 modified, 1 populated

---

## Phase 3: Scroll Region Implementation

### Stage 9: Add scroll region state
**Goal:** Add scroll region tracking fields without changing behavior.

**Tasks:**
1. Add fields to `TerminalPerformer`:
   - `scroll_top: usize`
   - `scroll_bottom: usize`
2. Initialize in `TerminalBuffer::process()` when creating performer:
   - `scroll_top: 0`
   - `scroll_bottom: height - 1`
3. Pass through resize (reset to full screen)

**Verification:**
- `cargo check` passes
- `cargo test terminal` passes
- No behavior change yet

**Files changed:** 2 modified

---

### Stage 10: Implement scroll_up and scroll_down helpers
**Goal:** Add scroll helper methods in `handlers/scroll.rs`.

**Tasks:**
1. Create `scroll.rs` with:
   - `scroll_up(&mut self, n: usize)` - scroll within region
   - `scroll_down(&mut self, n: usize)` - scroll within region
2. These use `self.scroll_top` and `self.scroll_bottom`
3. No handlers wired up yet

**Verification:**
- `cargo check` passes
- `cargo test terminal` passes

**Files changed:** 1 populated

---

### Stage 11: Implement CSI r handler (DECSTBM)
**Goal:** Handle scroll region set command.

**Tasks:**
1. Add `handle_set_scroll_region(&mut self, top: usize, bottom: usize)` to `scroll.rs`
2. Wire up 'r' case in `csi_dispatch`
3. Add unit tests for:
   - Default params (full screen)
   - Explicit params
   - Invalid params (top >= bottom)
   - Cursor moves to home after set

**Verification:**
- `cargo check` passes
- `cargo test terminal::tests::scroll` passes
- `cargo test terminal` passes

**Files changed:** 2 modified

---

### Stage 12: Implement CSI S and CSI T handlers
**Goal:** Handle explicit scroll commands.

**Tasks:**
1. Add `handle_scroll_up(&mut self, n: usize)` - CSI S handler
2. Add `handle_scroll_down(&mut self, n: usize)` - CSI T handler
3. Wire up 'S' and 'T' cases in `csi_dispatch`
4. Add unit tests for scroll within region

**Verification:**
- `cargo check` passes
- `cargo test terminal::tests::scroll` passes
- `cargo test terminal` passes

**Files changed:** 2 modified

---

### Stage 13: Update line_feed and reverse_index
**Goal:** Make existing scroll operations respect scroll region.

**Tasks:**
1. Update `line_feed()` to use `scroll_bottom` instead of `height`
2. Update `handle_reverse_index()` (ESC M) to use `scroll_top`
3. Add tests for scroll behavior at region boundaries

**Verification:**
- `cargo check` passes
- `cargo test terminal::tests::scroll` passes
- `cargo test terminal` passes
- Existing scroll tests still pass

**Files changed:** 2 modified

---

## Phase 4: Test Migration and Fixture

### Stage 14: Migrate tests to separate files
**Goal:** Move tests to `src/terminal/tests/` directory.

**Tasks:**
1. Move cursor tests to `src/terminal/tests/cursor_tests.rs`
2. Move scroll tests to `src/terminal/tests/scroll_tests.rs`
3. Move editing tests to `src/terminal/tests/editing_tests.rs`
4. Move style tests to `src/terminal/tests/style_tests.rs`
5. Move integration tests to `src/terminal/tests/integration_tests.rs`
6. Keep only tests requiring private access inline (if any)
7. Update `src/terminal/tests/mod.rs` to include all test modules

**Verification:**
- `cargo check` passes
- `cargo test terminal` passes
- All test counts match before/after

**Files changed:** 6 populated

---

### Stage 15: Create scroll region test fixture
**Goal:** Extract anonymous test fixture for CI.

**Tasks:**
1. Create `tests/fixtures/` directory if not exists
2. Create `scroll_region_test.cast` with:
   - Diverse scroll region sequences
   - No personal paths
   - Header documentation
3. Create integration test using fixture
4. Remove reference to personal path in `visual_comparison_with_real_cast` test

**Verification:**
- `cargo test terminal::tests::integration` passes
- Fixture file is anonymous and reusable
- CI passes

**Files changed:** 2 new, 1 modified

---

## Verification Checklist

After all stages complete:

- [x] `cargo check` passes
- [x] `cargo test` passes (all tests)
- [x] `cargo clippy` passes
- [x] `cargo fmt --check` passes
- [x] Visual comparison test with fixture passes
- [x] No personal paths in codebase
- [x] Public API unchanged (backward compatible)
- [x] Unhandled sequences produce debug logs

---

## Stage Dependencies

```
Stage 1 (structure)
    |
    v
Stage 2 (types) --> Stage 3 (buffer) --> Stage 4 (performer)
                                              |
                    +-------------------------+-------------------------+
                    |                         |                         |
                    v                         v                         v
              Stage 5 (cursor)          Stage 6 (editing)         Stage 7 (style)
                    |                         |                         |
                    +-------------------------+-------------------------+
                                              |
                                              v
                                        Stage 8 (observability)
                                              |
                                              v
                                        Stage 9 (scroll state)
                                              |
                                              v
                                        Stage 10 (scroll helpers)
                                              |
                                              v
                                        Stage 11 (CSI r)
                                              |
                                              v
                                        Stage 12 (CSI S/T)
                                              |
                                              v
                                        Stage 13 (line_feed/RI)
                                              |
                                              v
                                        Stage 14 (test migration)
                                              |
                                              v
                                        Stage 15 (fixture)
```

---

## Notes for Implementer

1. **Run tests after every stage** - Don't batch stages together
2. **Commit after each stage** - Enables easy rollback if needed
3. **Keep public API stable** - Re-exports must match original
4. **Scroll region is 0-indexed internally** - Convert from 1-indexed params
5. **Watch for off-by-one errors** - Common in scroll region math
6. **The visual_comparison test uses a personal path** - Must be replaced in Stage 15
7. **Module location is `src/terminal/`** - Top-level, not under player/
8. **Update imports in these files:**
   - `src/lib.rs` - Add `pub mod terminal;`
   - `src/player/mod.rs` - Re-export from `crate::terminal`
   - `src/player/native.rs` - Import from `crate::terminal`
   - `src/tui/widgets/file_explorer.rs` - Import from `crate::terminal`
