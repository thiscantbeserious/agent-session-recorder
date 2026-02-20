# Sub-ADR: player -- 2 Violations

Parent: [ADR.md](ADR.md)

## Scope

File: `src/player/render/viewport.rs` (437 lines)
Violations: 2 functions, combined score 71

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `compute()` | 25 | `render_viewport()` | 25 | 44 |
| `render_diff()` | 128 | `render_single_line()` | 128 | 27 |

## Function Analysis

### 1. `render_viewport()` at line 25 -- score 44

**Signature:**
```rust
#[allow(clippy::too_many_arguments)]
pub fn render_viewport(
    stdout: &mut io::Stdout, buffer: &TerminalBuffer,
    row_offset: usize, col_offset: usize,
    view_rows: usize, view_cols: usize,
    highlight_line: Option<usize>,
) -> Result<()>
```

**Current structure:**
Lines 25-112. For each `view_row` in `0..view_rows`:
1. Move cursor to line start (line 42)
2. `if is_highlighted` set highlight style (lines 45-47)
3. `if let Some(row) = buffer.row(buf_row)` (line 51) -- the row-exists branch:
   - Inner column loop `for view_col in 0..view_cols` (line 55):
     - `if buf_col < row.len()` (line 58) -- cell exists:
       - `if !is_highlighted && cell.style != current_style` (line 61): reset + apply ANSI codes
       - `else if is_highlighted && !in_highlight_style` (line 69): re-apply highlight
       - Push cell char (line 74)
     - `else` (lines 76-84) -- past end of row:
       - `if !is_highlighted && current_style != CellStyle::default()` (line 78): reset style
       - Push space
   - Post-row reset: `if current_style != CellStyle::default() || is_highlighted` (lines 88-90)
4. `else` (lines 91-104) -- empty row:
   - `if is_highlighted` / `else` for filling with spaces and optional reset

**Why complexity is high:** Four nesting levels: row loop > row exists > column loop > cell exists. Within the cell-exists branch, the highlight/style tracking adds more conditionals. The empty-row branch also has a conditional for highlight mode.

**Borrow checker constraint:** Free function. All data passed by reference. No borrow issues.

### 2. `render_single_line()` at line 128 -- score 27

**Signature:**
```rust
#[allow(clippy::too_many_arguments)]
pub fn render_single_line(
    stdout: &mut io::Stdout, buffer: &TerminalBuffer,
    buf_row: usize, view_row_offset: usize,
    col_offset: usize, view_cols: usize,
    is_highlighted: bool,
) -> Result<()>
```

**Current structure:**
Lines 128-194. Nearly identical to the inner body of `render_viewport()`'s row loop:
1. Early return if `buf_row < view_row_offset` (lines 138-140)
2. Calculate screen row (line 141)
3. Move cursor, set highlight (lines 146-149)
4. `if let Some(row) = buffer.row(buf_row)` (line 152):
   - Column loop with same cell-exists / past-end branching and style tracking as `render_viewport()` (lines 155-177)
   - Post-row reset (lines 179-181)
5. `else` -- empty row with spaces and optional highlight reset (lines 182-190)

**Why complexity is high:** Same structure as `render_viewport()`'s inner loop. The column loop with style tracking contributes the nesting.

**Shared logic between both functions:** The column-rendering logic (iterate columns, check cell existence, apply/track styles, handle highlight vs. normal, fill spaces past content, reset at end) is nearly identical. A shared `render_row()` helper would serve both.

**Extraction targets:**

1. `render_row(output: &mut String, row: Option<&[Cell]>, col_offset: usize, view_cols: usize, is_highlighted: bool)` -- free function. Renders a single row's content to the output string. Handles:
   - The `if let Some(row)` / else for empty rows
   - The column loop with style tracking
   - Highlight vs. normal style application
   - End-of-row reset

   This covers:
   - Lines 51-104 in `render_viewport()` (the inner body of the row loop, after cursor positioning)
   - Lines 152-190 in `render_single_line()` (the row rendering, after cursor positioning)

With this extraction:
- `render_viewport()` becomes: allocate string, for each row: position cursor, set highlight prefix, call `render_row()`, write to stdout.
- `render_single_line()` becomes: early return check, allocate string, position cursor, set highlight prefix, call `render_row()`, write to stdout.
- Both should be well below 15 complexity.

Note: `render_row()` needs access to the ANSI style functions (`style_to_ansi_fg`, `style_to_ansi_bg`, `style_to_ansi_attrs`) which are already module-level imports. The `CellStyle` tracking state is local to each row, so no cross-row state complicates the extraction.

## Dependencies

- `render_viewport()` is called from the player's main rendering loop.
- `render_single_line()` is called from the player's free-mode highlight update.
- Both use `style_to_ansi_fg`, `style_to_ansi_bg`, `style_to_ansi_attrs` from `crate::player::render::ansi`.
- Both use `CellStyle` and `TerminalBuffer` from `crate::terminal`.
- Neither calls the other.

## Testability Assessment

**Existing tests (lines 196-437):**
- `render_viewport_*`: 12 tests covering empty buffer, content, offsets, highlight, small view, larger-than-buffer, colors, bold, multiline. All assert `result.is_ok()` (no-panic tests).
- `render_single_line_*`: 11 tests covering empty, content, highlight, above-viewport, within-viewport, col offset, colors, narrow view, past content, row beyond buffer.

These tests exercise both functions through their public API. The refactored code (delegating to `render_row()`) will be covered by the same tests.

**Additional tests for `render_row()`:** The extracted function operates on a `String` output buffer, making it more testable than the original stdout-writing functions. However, writing new tests is out of scope.

**TDD approach:** Run `cargo test viewport` before and after extraction. Also run `cargo test snapshot_player_test` since the snapshot tests verify rendering output.
