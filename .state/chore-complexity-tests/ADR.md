# Sub-ADR: tests -- 1 Violation

Parent: [ADR.md](ADR.md)

## Scope

File: `tests/integration/snapshot_player_test.rs`
Violations: 1 function, score 21

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `test_playback()` | 327 | `render_viewport_snapshot()` | 327 | 21 |

Note: SonarCloud reports the name as `test_playback()` but the actual function at line 327 is `render_viewport_snapshot()`, a helper function within the test module (not a `#[test]` function itself).

## Function Analysis

### `render_viewport_snapshot()` at line 327 -- score 21

**Signature:**
```rust
fn render_viewport_snapshot(
    buffer: &TerminalBuffer,
    row_offset: usize, col_offset: usize,
    view_rows: usize, view_cols: usize,
    highlight_line: Option<usize>,
) -> String
```

**Current structure:**
Lines 327-382. Builds a snapshot string for testing:
1. Header lines with viewport dimensions, highlight info, buffer size (lines 335-348)
2. Row loop `for view_row in 0..view_rows` (line 350):
   - `is_highlighted` check (line 352)
   - Highlight prefix: `if is_highlighted { ">>> " } else { "    " }` (lines 354-358)
   - `if let Some(row) = buffer.row(buf_row)` (line 360) -- row exists:
     - Column loop `for view_col in 0..view_cols` (line 361):
       - `if buf_col < row.len()` (line 363) -- push cell char
       - `else` (lines 365-366) -- push space
   - `else` (lines 369-372) -- empty row, fill spaces
   - Highlight suffix: `if is_highlighted { " <<<" }` (lines 375-377)
   - Newline (line 378)

**Why complexity is high:** Three nesting levels: row loop > row exists > column loop > cell exists. Plus the highlight prefix/suffix conditionals add branching.

**Borrow checker constraint:** Free function, no issues. All parameters are references.

**Extraction targets:**

1. `render_snapshot_row(output: &mut String, buffer: &TerminalBuffer, buf_row: usize, col_offset: usize, view_cols: usize, is_highlighted: bool)` -- free function in the test module. Covers lines 354-378 (the inner body of the row loop). Handles: highlight prefix, row content rendering (cell-exists check with fallback to space), highlight suffix, newline.

With this extraction, `render_viewport_snapshot()` becomes: build header, for each row call `render_snapshot_row()`, return output. This should reduce complexity well below 15.

**Constraint:** This is a test file. The extraction must NOT alter any test assertions or test logic. `render_viewport_snapshot()` is a helper function (not a `#[test]`), so extracting from it into another helper is safe as long as the output format is identical.

## Dependencies

- `render_viewport_snapshot()` is called by several `#[test]` functions in the same file to produce snapshot strings for `insta::assert_snapshot!()`.
- It uses `TerminalBuffer::row()` and cell access.
- No other functions in this file are flagged.

## Testability Assessment

**Existing tests:** The function IS a test helper. The `#[test]` functions that call it serve as the regression tests. Running `cargo test snapshot_player_test` exercises all call sites.

**TDD approach:** Run `cargo test snapshot_player_test` before and after extraction. All snapshot assertions must produce identical output. The `insta` snapshot files will verify this -- any output change will cause a snapshot mismatch failure.
