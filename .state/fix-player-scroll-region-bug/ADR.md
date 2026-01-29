# ADR: Fix Player Scroll Region Bug

## Status
Accepted

## Context

Our native player's `TerminalBuffer` (in `src/player/terminal.rs`) uses the `vte` crate to parse ANSI escape sequences. While `vte` correctly parses scroll-related sequences, our `csi_dispatch` handler silently ignores them with a `_ => {}` catch-all.

This causes visual output to differ from standard terminal emulators when playing recordings that use scroll regions (common in TUI apps like vim, tmux, codex CLI, etc.).

### Current State

The `terminal.rs` file is ~1680 lines containing:
- Data types (Color, CellStyle, Cell, StyledLine)
- `TerminalBuffer` struct with VTE parser
- `TerminalPerformer` struct with all escape sequence handlers inline in match statements
- ~970 lines of tests

```rust
// src/player/terminal.rs, csi_dispatch()
match action {
    'A' => { /* cursor up - inline */ }
    'B' => { /* cursor down - inline */ }
    // ... 20+ handlers inline in match arms ...
    _ => {} // PROBLEM: ignores 'r', 'S', 'T' and provides no observability
}
```

Missing handlers:
- `'r'` - DECSTBM (Set Top and Bottom Margins)
- `'S'` - SU (Scroll Up)
- `'T'` - SD (Scroll Down)

The existing `ESC M` (Reverse Index) handler also doesn't respect scroll regions.

### Requirements Summary

1. **Bugfix:** Implement scroll region commands (r, S, T) and update existing scroll behavior
2. **Refactor:** Extract handlers into methods, organize in a central module structure
3. **Observability:** Log unhandled sequences for future gap detection
4. **Test Infrastructure:** Extract anonymous test fixture for scroll region verification

## Decision

### Architecture: Submodule Split (Option B)

Promote `terminal.rs` from `src/player/` to a **top-level module** at `src/terminal/`. The terminal emulator is a general-purpose VT emulator, not player-specific - it's already used by TUI widgets for previews and could serve future analysis features.

```
src/terminal/
├── mod.rs              # Public API, re-exports, TerminalBuffer
├── types.rs            # Color, CellStyle, Cell, StyledLine
├── performer.rs        # TerminalPerformer struct + VTE trait impl (thin dispatch)
├── handlers/
│   ├── mod.rs          # Handler utilities, unhandled sequence logging
│   ├── cursor.rs       # Cursor movement (A, B, C, D, H, f, G, d, s, u, ESC 7/8)
│   ├── scroll.rs       # Scroll region (r, S, T, ESC M) + scroll_up/scroll_down
│   ├── editing.rs      # Erase/delete (J, K, L, M, P, @, X)
│   └── style.rs        # SGR handler (m)
└── tests/
    ├── mod.rs          # Test module root
    ├── cursor_tests.rs
    ├── scroll_tests.rs
    ├── editing_tests.rs
    ├── style_tests.rs
    └── integration_tests.rs
```

**Import updates required:**
- `src/player/mod.rs`: Re-export from `crate::terminal` instead of local
- `src/player/native.rs`: Import from `crate::terminal`
- `src/lib.rs`: Add `pub mod terminal;` and update re-exports
- `src/tui/widgets/file_explorer.rs`: Import from `crate::terminal`

### Module Responsibilities

**`mod.rs`** - Public interface
- Re-exports all public types (Cell, CellStyle, Color, StyledLine, TerminalBuffer)
- Contains `TerminalBuffer` struct implementation
- Maintains backward compatibility with existing API

**`types.rs`** - Data structures
- `Color` enum (Default, Named, Indexed, RGB)
- `CellStyle` struct (fg, bg, bold, dim, italic, underline, reverse)
- `Cell` struct (char, style)
- `StyledLine` struct

**`performer.rs`** - VTE integration
- `TerminalPerformer` struct definition
- `impl Perform for TerminalPerformer` with thin dispatch to handlers
- Scroll region state (`scroll_top`, `scroll_bottom`)

**`handlers/mod.rs`** - Handler coordination
- Common utilities for parameter parsing
- `log_unhandled_csi()` and `log_unhandled_esc()` for observability

**`handlers/cursor.rs`** - Cursor handlers
```rust
impl TerminalPerformer<'_> {
    pub(crate) fn handle_cursor_up(&mut self, n: usize);
    pub(crate) fn handle_cursor_down(&mut self, n: usize);
    pub(crate) fn handle_cursor_forward(&mut self, n: usize);
    pub(crate) fn handle_cursor_back(&mut self, n: usize);
    pub(crate) fn handle_cursor_position(&mut self, row: usize, col: usize);
    pub(crate) fn handle_cursor_horizontal_absolute(&mut self, col: usize);
    pub(crate) fn handle_cursor_vertical_absolute(&mut self, row: usize);
    pub(crate) fn handle_save_cursor(&mut self);
    pub(crate) fn handle_restore_cursor(&mut self);
}
```

**`handlers/scroll.rs`** - Scroll handlers (NEW)
```rust
impl TerminalPerformer<'_> {
    pub(crate) fn handle_set_scroll_region(&mut self, top: usize, bottom: usize);
    pub(crate) fn scroll_up(&mut self, n: usize);
    pub(crate) fn scroll_down(&mut self, n: usize);
    pub(crate) fn handle_scroll_up(&mut self, n: usize);   // CSI S
    pub(crate) fn handle_scroll_down(&mut self, n: usize); // CSI T
    pub(crate) fn handle_reverse_index(&mut self);         // ESC M
}
```

**`handlers/editing.rs`** - Editing handlers
```rust
impl TerminalPerformer<'_> {
    pub(crate) fn handle_erase_display(&mut self, mode: u16);
    pub(crate) fn handle_erase_line(&mut self, mode: u16);
    pub(crate) fn handle_delete_lines(&mut self, n: usize);
    pub(crate) fn handle_insert_lines(&mut self, n: usize);
    pub(crate) fn handle_delete_chars(&mut self, n: usize);
    pub(crate) fn handle_insert_chars(&mut self, n: usize);
    pub(crate) fn handle_erase_chars(&mut self, n: usize);
}
```

**`handlers/style.rs`** - SGR handler
```rust
impl TerminalPerformer<'_> {
    pub(crate) fn handle_sgr(&mut self, params: &[u16]);
}
```

### Scroll Region Implementation

Add two fields to `TerminalPerformer`:
```rust
struct TerminalPerformer<'a> {
    // ... existing fields ...
    /// Top margin of scroll region (0-indexed, inclusive)
    scroll_top: usize,
    /// Bottom margin of scroll region (0-indexed, inclusive)
    scroll_bottom: usize,
}
```

**CSI r (DECSTBM):**
```rust
fn handle_set_scroll_region(&mut self, top: usize, bottom: usize) {
    // Convert 1-indexed params to 0-indexed, clamp to valid range
    self.scroll_top = top.saturating_sub(1).min(self.height - 1);
    self.scroll_bottom = bottom.saturating_sub(1).min(self.height - 1);

    // Ensure top < bottom, else reset to full screen
    if self.scroll_top >= self.scroll_bottom {
        self.scroll_top = 0;
        self.scroll_bottom = self.height - 1;
    }

    // Move cursor to home (per DECSTBM spec)
    *self.cursor_row = 0;
    *self.cursor_col = 0;
}
```

**scroll_up / scroll_down:**
```rust
fn scroll_up(&mut self, n: usize) {
    for _ in 0..n {
        self.buffer.remove(self.scroll_top);
        self.buffer.insert(self.scroll_bottom, vec![Cell::default(); self.width]);
    }
}

fn scroll_down(&mut self, n: usize) {
    for _ in 0..n {
        self.buffer.remove(self.scroll_bottom);
        self.buffer.insert(self.scroll_top, vec![Cell::default(); self.width]);
    }
}
```

**Update line_feed:**
```rust
fn line_feed(&mut self) {
    if *self.cursor_row < self.scroll_bottom {
        *self.cursor_row += 1;
    } else if *self.cursor_row == self.scroll_bottom {
        self.scroll_up(1);
    }
    // If cursor is below scroll region, just move down (no scroll)
}
```

**Update reverse_index (ESC M):**
```rust
fn handle_reverse_index(&mut self) {
    if *self.cursor_row > self.scroll_top {
        *self.cursor_row -= 1;
    } else if *self.cursor_row == self.scroll_top {
        self.scroll_down(1);
    }
    // If cursor is above scroll region, just move up (no scroll)
}
```

### Observability

Add tracing for unhandled sequences:
```rust
// In handlers/mod.rs
pub(crate) fn log_unhandled_csi(action: char, params: &[u16], intermediates: &[u8]) {
    tracing::debug!(
        action = %action,
        params = ?params,
        intermediates = ?intermediates,
        "Unhandled CSI sequence"
    );
}

pub(crate) fn log_unhandled_esc(byte: u8, intermediates: &[u8]) {
    tracing::debug!(
        byte = %byte,
        intermediates = ?intermediates,
        "Unhandled ESC sequence"
    );
}
```

### Test Organization

**Integration tests** (`tests/integration_tests.rs`):
- Visual comparison with pyte using extracted fixture
- Full sequence replay tests
- TUI app compatibility tests

**Unit tests by handler group:**
- `cursor_tests.rs` - Cursor movement edge cases
- `scroll_tests.rs` - Scroll region behavior
- `editing_tests.rs` - Erase/delete operations
- `style_tests.rs` - SGR color/attribute parsing

**Inline tests (minimal):**
- Only tests requiring private field access that can't be tested via public API

### Test Fixture

Extract anonymous scroll region test fixture:
```
tests/fixtures/scroll_region_test.cast
```

Requirements:
- Contains diverse scroll region sequences (CSI r, S, T, ESC M)
- No personal paths or identifying information
- Includes expected output for verification
- Documented purpose in header comments

## Consequences

### Positive
- Clear code organization following project patterns (asciicast module)
- Each handler group is focused and testable in isolation
- Observability for future sequence gaps
- Playback matches asciinema/standard terminal emulators
- TUI app recordings render correctly
- Scalable architecture for adding new handlers

### Negative
- More files to navigate (mitigated by clear structure)
- Refactor effort before the bugfix lands
- Need to maintain re-exports for backward compatibility

### Risks (Low)
- Regression during refactor - mitigated by keeping tests passing at each stage
- Performance impact of more function calls - negligible for terminal emulation

## Architect Review Notes

**Reviewed:** 2026-01-29

**Assessment:** The design aligns with existing codebase patterns (asciicast module structure) and appropriately balances organization with implementation effort.

**Implementation Notes:**
1. The `scroll_up`/`scroll_down` methods should be on `TerminalPerformer` to match existing patterns
2. Scroll region fields stored in `TerminalPerformer` and initialized from `TerminalBuffer` dimensions
3. The `line_feed` method must be updated to respect scroll regions for full correctness
4. Integration tests should use the extracted fixture, not personal paths

**Known Limitations (acceptable):**
- DECOM (origin mode) not implemented - cursor positioning remains absolute
- Some edge cases may differ slightly from xterm/vt100 behavior

**Technical Risks (low):**
- Performance impact negligible for typical recordings
- Edge cases well-defined by DECSTBM spec
