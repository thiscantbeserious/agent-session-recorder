# Sub-ADR: tui/widgets -- 1 Violation

Parent: [ADR.md](ADR.md)

## Scope

File: `src/tui/widgets/file_explorer.rs` (1260+ lines)
Violations: 1 function, score 79

## SonarCloud-to-Source Mapping (verified)

| SonarCloud Name | SonarCloud Line | Actual Function | Actual Line | Score |
|-----------------|-----------------|-----------------|-------------|-------|
| `handle_input()` | 958 | `FileExplorerWidget::render()` (Widget trait impl) | 958 | 79 |

## Function Analysis

### `render()` at line 958 -- score 79

**Signature:**
```rust
impl Widget for FileExplorerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer)
```

**Current structure:**
Lines 958-1242. The function has three major blocks:

1. **Layout and item data collection** (lines 958-984): Splits area, collects visible items into `item_data: Vec<(String, String, String, String, bool, bool, bool)>`.

2. **Item span construction** (lines 989-1080): The `.map()` closure iterating over `item_data` is the biggest complexity contributor. For each item, it:
   - Optionally adds checkbox span (lines 995-998)
   - Adds time prefix span (lines 1001-1004)
   - Adds backup indicator (lines 1007-1009)
   - **Rename branch** (lines 1012-1056): `if idx == selected_idx { if let Some((input, cursor, selected_all)) = rename_state { ... } }` with deeply nested:
     - `if selected_all { ... } else { ... }` (line 1017)
     - Inside else: `if !before.is_empty()` (line 1024), `if cursor < input.len()` (line 1027) with inner `if !after.is_empty()` (line 1040), and `else` for cursor-at-end (lines 1044-1048)
   - **Normal item branch** (lines 1059-1077): lock-based style selection, recording indicator, metadata spans.

3. **Preview panel rendering** (lines 1130-1241): `if self.show_preview && chunks.len() > 1` wraps a large block with:
   - `if let Some((name, agent, size, modified, path, lock_data)) = preview_data` (line 1132)
   - `if let Some((duration, markers, styled_preview)) = session_preview_data` (line 1149) with nested `if has_backup` (line 1159), `if let Some(ref lock) = lock_data` (line 1171), `if !styled_preview.is_empty()` (line 1189)
   - `else` fallback for no session preview (lines 1213-1226)
   - `else` for no file selected (lines 1229-1231)

**Why complexity is high:** The item-building closure has 5+ levels of nesting from the rename state branch. The preview panel has 4 levels of nesting from chained `if let` / `if` conditions.

**Borrow checker constraint:** `render(self, ...)` consumes `self` (moved). The item data is pre-collected to avoid borrow issues with `self.explorer` during the `.map()`. Extracted helpers must be **free functions** since `self` is consumed and fields are accessed through locals. The `rename_state: Option<(&str, usize, bool)>` contains a `&str` reference -- the extracted function must accept this reference as a parameter.

**Extraction targets:**

1. `build_rename_item_spans(input: &str, cursor: usize, selected_all: bool, agent: &str, size_str: &str, theme: &Theme) -> Vec<Span>` -- free function. Covers lines 1013-1055 (the entire rename-state rendering block). Takes the rename state tuple fields and returns the span vector. This eliminates the deepest nesting. The function handles:
   - `selected_all` branch: cursor-style text + `.cast` suffix
   - Normal edit branch: before-cursor text, cursor character with blink style, after-cursor text, `.cast` suffix

2. `build_normal_item_spans(name: &str, agent: &str, size_str: &str, is_locked: bool, is_checked: bool, show_checkboxes: bool, time_str: &str, has_bak: bool, theme: &Theme) -> Vec<Span>` -- free function. Covers lines 994-1077 minus the rename path. Builds the standard item spans including checkbox, time prefix, backup indicator, lock indicator, and metadata. This is optional -- it may not reduce complexity enough on its own since the remaining normal path has low branching.

3. `render_preview_panel(buf: &mut Buffer, area: Rect, preview_data: Option<(...)>, session_preview_data: Option<(...)>, has_backup: bool, theme: &Theme)` -- free function. Covers lines 1130-1241. Takes the pre-collected preview data and renders the preview panel. This is a substantial block but the nesting is mostly from `if let` guards which are hard to simplify beyond extraction.

**Minimum viable extraction:** Target (1) alone may reduce the score sufficiently since the rename branch contributes the majority of the nesting. If the score is still > 15 after extracting (1), add (3) for the preview panel.

## Dependencies

- `render()` is called by the ratatui framework through the `Widget` trait.
- It references `self.explorer`, `self.show_checkboxes`, `self.rename_state`, `self.session_preview`, `self.has_backup`, and `self.show_preview`.
- No other functions in this file are flagged.

## Testability Assessment

**Existing tests:** `tests/integration/snapshot_tui_test.rs` contains snapshot tests for the file explorer rendering. These are the primary regression safety net.

**TDD approach:** `render()` is a Widget trait method requiring a `Buffer` to render into. It cannot be easily unit-tested in isolation. The snapshot integration tests are the baseline. Extracted free functions (like `build_rename_item_spans()`) would be independently testable but writing tests is out of scope for this pure refactoring.
