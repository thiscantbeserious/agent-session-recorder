# Sub-ADR: tui/cleanup_app -- 1 Violation

Parent: [ADR.md](ADR.md)

## Scope

File: `src/tui/cleanup_app.rs`
Violations: 1 function, score 16

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `handle_input()` | 190 | `CleanupApp::select_by_glob()` | 190 | 16 |

## Function Analysis

### `select_by_glob()` at line 190 -- score 16

**Signature:**
```rust
fn select_by_glob(&mut self, pattern: &str) -> usize
```

**Current structure:**
Lines 190-248. The function:
1. Parses agent/pattern syntax with `if let Some(slash_pos) = pattern.find('/')` (lines 192-198): splits `agent_filter` and `file_pattern`.
2. Collects visible items into `items_to_select: Vec<(usize, String, String, bool, bool)>` (lines 201-214).
3. Iterates collected items (lines 221-239):
   - `if is_locked { continue; }` (lines 222-224)
   - `let matches = if let Some(agent_pat) = agent_filter { glob_match(...) && glob_match(...) } else { glob_match(...) }` (lines 225-229) -- complexity from `if let` + boolean combination
   - `if matches && !is_selected { ... }` (lines 230-238) -- navigates to item, toggles selection
4. Restores original position (lines 242-245).

**Why complexity is high:** Score 16 barely exceeds the threshold. The branching comes from the `if let` for agent filter parsing, the `if is_locked` skip, the `if let Some(agent_pat)` match condition, and the `if matches && !is_selected` guard. The navigation loop (`for _ in 0..vis_idx`) adds a nesting increment.

**Borrow checker constraint:** `&mut self`, no conflicting borrows. The `items_to_select` vector is pre-collected to avoid borrowing `self.shared.explorer` during mutation. Extracted helpers can be free functions or methods.

**Extraction targets:**

1. `matches_glob_pattern(agent: &str, name: &str, agent_filter: Option<&str>, file_pattern: &str) -> bool` -- free function. Covers lines 225-229. Takes the item's agent/name and the parsed filter, returns whether the item matches. This removes the `if let Some(agent_pat)` nesting from the loop body.

This single extraction should bring the score from 16 to below 15 by removing one nesting level inside the loop. If not sufficient, the parse step (lines 192-198) can be extracted into `parse_glob_pattern(pattern: &str) -> (Option<&str>, &str)`, but this is unlikely to be needed.

## Dependencies

- `select_by_glob()` is called from `handle_glob_input_key()` (around line 161) when the user presses Enter in glob input mode.
- It calls `glob_match()` (a free function elsewhere in the file or module) and methods on `self.shared.explorer`.

## Testability Assessment

**Existing tests:** No dedicated unit tests for `select_by_glob()` in the file. The function is exercised through TUI integration tests.

**TDD approach:** Full `cargo test` is the baseline. The extracted `matches_glob_pattern()` function is a pure function and could be unit-tested, but writing new tests is out of scope.
