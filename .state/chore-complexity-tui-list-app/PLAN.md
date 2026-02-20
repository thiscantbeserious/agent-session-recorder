# Plan: Reduce Cognitive Complexity for SonarCloud Quality Gate -- Round 2

References: [ADR.md](ADR.md) and per-module sub-ADRs

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. `ListApp::draw()` (score 80) borrows `self.app` mutably via `self.app.draw(|frame| {...})`
   while the closure captures many `self` fields. Extracted helpers must be either free
   functions or static methods taking explicit parameters -- they cannot take `&self` because
   `self.app` is already mutably borrowed. PR #141 solved this exact pattern for
   `CleanupApp::draw()`; reuse the same approach.
   **Details:** [ADR-tui-list-app.md](ADR-tui-list-app.md) section 1.

2. `FileExplorerWidget::render()` (score 79) uses a closure inside `.map()` that builds
   `ListItem` spans. The closure captures `show_checkboxes`, `selected_idx`, `rename_state`,
   and `theme`. Extracting the closure body into a free function requires passing these
   as explicit parameters. Verify that the extracted function compiles without lifetime
   issues from the `rename_state: Option<(&str, usize, bool)>` reference.
   **Details:** [ADR-tui-widgets.md](ADR-tui-widgets.md).

3. `EventHandler::new()` (score 43) -- the complexity is inside a `thread::spawn` closure.
   Extracting a helper function requires the helper to be `Send + 'static` since it runs
   in the spawned thread. A free function `poll_events(tx, running, tick_rate)` is the
   cleanest extraction pattern.
   **Details:** [ADR-tui-event-bus.md](ADR-tui-event-bus.md).

4. The REQUIREMENTS lists `test_playback()` at line 327 in `snapshot_player_test.rs`,
   but the actual function at that line is `render_viewport_snapshot()`. SonarCloud
   function names do not always match Rust source names. The implementer must verify which
   function is flagged and refactor accordingly.
   **Details:** [ADR-tests.md](ADR-tests.md).

5. Several functions (`analyze::handle`, `config::handle_migrate`, `config::print_diff_preview`)
   are annotated `#[cfg(not(tarpaulin_include))]` and interact with stdin/stdout. The TDD
   "RED" phase for these means running `cargo test` as baseline only -- no new tests can
   exercise these functions directly.
   **Details:** [ADR-commands.md](ADR-commands.md).

6. REQUIREMENTS names 25 of 32 violations. The remaining 7 Medium-tier violations must be
   discovered via the SonarCloud dashboard or by running a local analysis. The implementer
   should run SonarCloud analysis on the branch early to identify all violations and update
   this PLAN with any additional functions.

7. `ContentCleaner` at line 53 in `cleaner.rs` -- SonarCloud reports `clean()` but the
   actual flagged function may be `ContentCleaner::new()` (the constructor with config-based
   character set building) rather than `handle_escape_char()`. The implementer must check the
   SonarCloud dashboard to determine which function is flagged and apply the appropriate
   extraction from [ADR-analyzer.md](ADR-analyzer.md).

## SonarCloud Function Name Mapping

SonarCloud reports function names that may differ from actual Rust function names. The
following maps REQUIREMENTS entries (by file:line) to actual function names (verified by
reading source):

| REQUIREMENTS entry | Actual function | Score | Sub-ADR |
|---|---|---|---|
| `list_app.rs:1426` `handle_input()` | `ListApp::draw()` | 80 | [tui-list-app](ADR-tui-list-app.md) |
| `file_explorer.rs:958` `handle_input()` | `FileExplorerWidget::render()` | 79 | [tui-widgets](ADR-tui-widgets.md) |
| `analyze.rs:34` `execute()` | `analyze::handle()` | 62 | [commands](ADR-commands.md) |
| `list_app.rs:1254` `handle_key()` | `ListApp::handle_mouse()` | 48 | [tui-list-app](ADR-tui-list-app.md) |
| `viewport.rs:25` `compute()` | `render_viewport()` | 44 | [player](ADR-player.md) |
| `event_bus.rs:45` `recv()` | `EventHandler::new()` | 43 | [tui-event-bus](ADR-tui-event-bus.md) |
| `migrate/mod.rs:147` `migrate_config()` | `sort_toml_text()` | 34 | [config](ADR-config.md) |
| `docs.rs:165` `format_field()` | `insert_optional_field_templates()` | 28 | [config](ADR-config.md) |
| `config.rs:243` `handle()` | `print_diff_preview()` | 28 | [commands](ADR-commands.md) |
| `chunk.rs:285` `create_chunks()` | `find_segments_for_range()` | 27 | [analyzer](ADR-analyzer.md) |
| `viewport.rs:128` `render_diff()` | `render_single_line()` | 27 | [player](ADR-player.md) |
| `cleaner.rs:53` `clean()` | `ContentCleaner::new()` or `handle_escape_char()` | 22 | [analyzer](ADR-analyzer.md) |
| `snapshot_player_test.rs:327` `test_playback()` | `render_viewport_snapshot()` | 21 | [tests](ADR-tests.md) |
| `extractor.rs:176` `extract()` | `redistribute_time()` | 20 | [analyzer](ADR-analyzer.md) |
| `list_app.rs:704` `handle_scroll()` | `handle_rename_input_key()` | 18 | [tui-list-app](ADR-tui-list-app.md) |
| `service.rs:378` `analyze()` | `AnalyzerService::analyze()` | 18 | [analyzer](ADR-analyzer.md) |
| `list_app.rs:1047` `process_event()` | `render_context_menu_modal()` | 18 | [tui-list-app](ADR-tui-list-app.md) |
| `config.rs:70` `run()` | `handle_migrate()` | 17 | [commands](ADR-commands.md) |
| `list_app.rs:213` `new()` | `handle_normal_key()` | 17 | [tui-list-app](ADR-tui-list-app.md) |
| `normalize.rs:115` `normalize()` | `EmptyLineFilter::transform()` | 16 | [analyzer](ADR-analyzer.md) |
| `aggressive.rs:579` `transform()` | `WindowedLineDeduplicator::flush_lines()` | 16 | [analyzer](ADR-analyzer.md) |
| `copy.rs:40` `copy_output()` | `Copy::file()` | 16 | [clipboard](ADR-clipboard.md) |
| `cleanup_app.rs:190` `handle_input()` | `CleanupApp::select_by_glob()` | 16 | [tui-cleanup-app](ADR-tui-cleanup-app.md) |
| `backend/mod.rs:338` `process()` | `extract_json()` | 16 | [analyzer](ADR-analyzer.md) |
| `normalize.rs:31` `normalize()` | `NormalizeWhitespace::transform()` | 16 | [analyzer](ADR-analyzer.md) |

## TDD Cycle for Pure Refactoring

Since this is pure refactoring (no new behavior), each function follows this cycle:

1. **RED/GREEN (test baseline)**: Verify `cargo test` passes before touching source code.
   For functions with existing dedicated tests, run those specifically. For functions without
   tests (TUI handlers, command handlers), the full test suite is the baseline.
2. **REFACTOR**: Extract helper functions to reduce complexity below 15. No behavioral changes.
3. **GREEN (regression check)**: Run the same tests again -- they must still pass.
4. **Format and lint**: `cargo fmt` and `cargo clippy`.

## Stages

### Stage 1: tui/list_app (5 violations as one unit)

Goal: Reduce complexity of all 5 flagged functions in `list_app.rs` to below 15.
Sub-ADR: [ADR-tui-list-app.md](ADR-tui-list-app.md)

Owner: implementer

All 5 functions must be refactored as a single sequential unit because they share the
same file and some share state patterns.

#### 1a. ListApp::draw() -- score 80

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test` -- full suite must pass (green baseline)
- [ ] Note: `draw()` is not directly unit-testable (requires terminal). Existing snapshot
      tests in `tests/integration/snapshot_tui_test.rs` are the regression safety net.

**REFACTOR -- extract helpers (see ADR-tui-list-app.md section 1 for line ranges):**
- [ ] Extract `render_status_line_for_mode()` as a free function for the status line
      `match mode` block (lines 1484-1579). The inner `Mode::RenameInput` arm delegates
      to `build_rename_status_spans()`.
- [ ] Extract `footer_text_for_mode()` as a free function returning `&str` for the footer
      `match mode` block (lines 1582-1607).
- [ ] Extract `render_modal_overlays()` as a free function for the modal overlay
      `match mode` block (lines 1610-1648).
- [ ] Target: `draw()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 1b. ListApp::handle_mouse() -- score 48

**RED/GREEN -- establish test baseline:**
- [ ] Verify `cargo test` passes as baseline

**REFACTOR -- extract helpers (see ADR-tui-list-app.md section 2):**
- [ ] Extract `handle_normal_mouse()` method for the `Mode::Normal` arm (lines 1258-1285).
- [ ] Extract `handle_context_menu_mouse()` method for the `Mode::ContextMenu` arm
      (lines 1287-1328).
- [ ] Extract `handle_confirm_modal_mouse()` method merging the confirm-delete/unlock arms
      (lines 1330-1367).
- [ ] Target: `handle_mouse()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 1c. handle_rename_input_key() -- score 18

**RED/GREEN -- establish test baseline:**
- [ ] Verify `cargo test` passes as baseline

**REFACTOR -- extract helpers (see ADR-tui-list-app.md section 3):**
- [ ] Extract `handle_rename_backspace()` method (lines 716-730).
- [ ] Extract `handle_rename_delete()` method (lines 732-741).
- [ ] Extract `handle_rename_char_input()` method (lines 770-796).
- [ ] Target: `handle_rename_input_key()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 1d. render_context_menu_modal() -- score 18

**RED/GREEN -- establish test baseline:**
- [ ] Verify `cargo test` passes as baseline

**REFACTOR -- extract helpers (see ADR-tui-list-app.md section 4):**
- [ ] Extract `build_menu_item_label()` free function (lines 1083-1093).
- [ ] Extract `menu_item_style()` free function (lines 1095-1101).
- [ ] Target: `render_context_menu_modal()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 1e. handle_normal_key() -- score 17

**RED/GREEN -- establish test baseline:**
- [ ] Verify `cargo test` passes as baseline

**REFACTOR -- extract helpers (see ADR-tui-list-app.md section 5):**
- [ ] Extract `redirect_if_locked()` method deduplicating 6 identical locked-check blocks
      (lines 217, 229, 237, 243, 250, 257).
- [ ] Target: `handle_normal_key()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

Files: `src/tui/list_app.rs`
Depends on: none

Considerations:
- All 5 functions share the same file. They must be done sequentially to avoid merge
  conflicts. Edit bottom-to-top (draw at 1426 first, then handle_mouse at 1254, etc.)
  so line-number shifts from earlier extractions do not affect later ones.
- `draw()` has the closure borrow problem: `self.app.draw(|frame| {...})`. Extracted helpers
  must be free functions taking explicit parameters, not `&self` methods.
- `handle_normal_key()` has a clear DRY opportunity: the locked-check pattern repeats 6
  times. Extracting it into one helper simultaneously reduces complexity and improves
  readability.

---

### Stage 2: tui/widgets + tui/event_bus + tui/cleanup_app

Goal: Reduce complexity of remaining TUI module violations.

Owner: implementer

#### 2a. FileExplorerWidget::render() -- score 79

Sub-ADR: [ADR-tui-widgets.md](ADR-tui-widgets.md)

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test` -- full suite must pass

**REFACTOR -- extract helpers:**
- [ ] Extract `build_rename_item_spans()` free function for the rename-state rendering
      block (lines 1013-1055).
- [ ] Extract `render_preview_panel()` free function for the preview panel block
      (lines 1130-1241) if needed to reach target.
- [ ] Target: `render()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 2b. EventHandler::new() -- score 43

Sub-ADR: [ADR-tui-event-bus.md](ADR-tui-event-bus.md)

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test event_bus` -- existing tests must pass

**REFACTOR -- extract helpers:**
- [ ] Extract `poll_events()` free function with signature
      `fn poll_events(tx: mpsc::Sender<Event>, running: Arc<AtomicBool>, tick_rate: Duration)`.
- [ ] Extract `dispatch_crossterm_event()` free function inside `poll_events()` mapping
      `CrosstermEvent` to `Event` and returning `bool` (false to break).
- [ ] Target: `new()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test event_bus` -- tests must still pass
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 2c. CleanupApp::select_by_glob() -- score 16

Sub-ADR: [ADR-tui-cleanup-app.md](ADR-tui-cleanup-app.md)

**RED/GREEN -- establish test baseline:**
- [ ] Verify `cargo test` passes as baseline

**REFACTOR -- extract helpers:**
- [ ] Extract `matches_glob_pattern()` free function for the match logic (lines 225-229).
- [ ] Target: `select_by_glob()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

Files: `src/tui/widgets/file_explorer.rs`, `src/tui/event_bus.rs`, `src/tui/cleanup_app.rs`
Depends on: none

Considerations:
- All Stage 2 sub-stages are in different files and can execute in parallel.

---

### Stage 3: commands (analyze.rs + config.rs)

Goal: Reduce complexity of all 3 flagged command handler functions.
Sub-ADR: [ADR-commands.md](ADR-commands.md)

Owner: implementer

#### 3a. analyze::handle() -- score 62

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] Note: `handle()` is `#[cfg(not(tarpaulin_include))]` and interacts with stdin/stdout.

**REFACTOR -- extract helpers:**
- [ ] Extract `build_analyze_options()` free function for option cascade (lines 79-108).
- [ ] Extract `apply_agent_config()` free function for per-agent config (lines 110-127).
- [ ] Extract `handle_curation()` free function for curation flow (lines 178-228).
      Mark `#[cfg(not(tarpaulin_include))]`.
- [ ] Extract `handle_rename_suggestion()` free function for rename flow (lines 236-272).
      Mark `#[cfg(not(tarpaulin_include))]`.
- [ ] Target: `handle()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 3b. config::print_diff_preview() -- score 28

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test` -- full suite must pass

**REFACTOR -- extract helpers:**
- [ ] Extract `handle_section_header_line()` free function for section header processing
      (lines 256-272).
- [ ] Extract `print_field_line()` free function for field assignment processing
      (lines 275-297).
- [ ] Target: `print_diff_preview()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

#### 3c. config::handle_migrate() -- score 17

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test` -- full suite must pass

**REFACTOR -- extract helpers:**
- [ ] Extract `print_migration_info()` free function for Case 3 display logic
      (lines 120-163).
- [ ] Target: `handle_migrate()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test` -- full suite must pass
- [ ] `cargo fmt` and `cargo clippy`

Files: `src/commands/analyze.rs`, `src/commands/config.rs`
Depends on: none

Considerations:
- 3a is in a different file from 3b/3c and can run in parallel with them.
- 3b and 3c share `src/commands/config.rs` and must be sequential (3b first at line 243,
  then 3c at line 70, to minimize line-number drift).

---

### Stage 4: player (viewport.rs)

Goal: Reduce complexity of both viewport rendering functions.
Sub-ADR: [ADR-player.md](ADR-player.md)

Owner: implementer

#### 4a. render_viewport() -- score 44 and render_single_line() -- score 27

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test viewport` -- existing tests must pass

**REFACTOR -- extract helpers:**
- [ ] Extract `render_row()` free function for the inner row-rendering logic shared by
      both functions. Takes `(output: &mut String, row: Option<&[Cell]>, col_offset, view_cols, is_highlighted)`.
- [ ] Refactor `render_viewport()` to call `render_row()` in its row loop.
- [ ] Refactor `render_single_line()` to delegate to `render_row()`.
- [ ] Target: both functions <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test viewport` -- tests must still pass
- [ ] Run `cargo test snapshot_player_test` -- snapshot tests must still pass
- [ ] `cargo fmt` and `cargo clippy`

Files: `src/player/render/viewport.rs`
Depends on: none

---

### Stage 5: config (migrate/mod.rs + docs.rs)

Goal: Reduce complexity of both config utility functions.
Sub-ADR: [ADR-config.md](ADR-config.md)

Owner: implementer

#### 5a. sort_toml_text() -- score 34

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test migrate` -- existing tests must pass

**REFACTOR -- extract helpers:**
- [ ] Extract `collect_section_blocks()` free function for block building (lines 171-191).
- [ ] Extract `reassemble_sorted_toml()` free function for reassembly (lines 206-232).
- [ ] Target: `sort_toml_text()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test migrate` -- tests must still pass
- [ ] `cargo fmt` and `cargo clippy`

#### 5b. insert_optional_field_templates() -- score 28

**RED/GREEN -- establish test baseline:**
- [ ] Run `cargo test docs` -- existing tests must pass

**REFACTOR -- extract helpers:**
- [ ] Extract `collect_present_fields()` free function (lines 168-192).
- [ ] Extract `insert_missing_for_section()` free function (lines 219-249).
- [ ] Target: `insert_optional_field_templates()` <= 15 complexity

**GREEN -- verify no regression:**
- [ ] Run `cargo test docs` -- tests must still pass
- [ ] `cargo fmt` and `cargo clippy`

Files: `src/config/migrate/mod.rs`, `src/config/docs.rs`
Depends on: none

Considerations:
- 5a and 5b are in different files and can run in parallel.

---

### Stage 6: analyzer (7 files, 9 violations)

Goal: Reduce complexity of all 9 flagged analyzer functions.
Sub-ADR: [ADR-analyzer.md](ADR-analyzer.md)

Owner: implementer

All sub-stages are in different files and can execute in parallel.

#### 6a. find_segments_for_range() in chunk.rs -- score 27

- [ ] Run `cargo test chunk` as baseline
- [ ] Extract `extract_partial_content()` free function (lines 326-355)
- [ ] Target: `find_segments_for_range()` <= 15 complexity
- [ ] Run `cargo test chunk` + `cargo fmt` + `cargo clippy`

#### 6b. ContentCleaner (cleaner.rs) -- score 22

- [ ] Run `cargo test content_cleaner_test` as baseline
- [ ] Check SonarCloud dashboard: is the flagged function `new()` (line 53) or
      `handle_escape_char()` (line 149)?
- [ ] If `new()`: extract `build_strip_chars()` free function
- [ ] If `handle_escape_char()`: extract `handle_csi_char()` and `handle_osc_char()` methods
- [ ] Target: all cleaner.rs functions <= 15 complexity
- [ ] Run `cargo test content_cleaner_test` + `cargo test analyzer_content_test` +
      `cargo fmt` + `cargo clippy`

#### 6c. redistribute_time() in extractor.rs -- score 20

- [ ] Run `cargo test extractor` as baseline
- [ ] Extract `measure_excess_time()` free function (lines 180-189)
- [ ] Target: `redistribute_time()` <= 15 complexity
- [ ] Run `cargo test extractor` + `cargo test analyzer_content_test` +
      `cargo fmt` + `cargo clippy`

#### 6d. AnalyzerService::analyze() in service.rs -- score 18

- [ ] Run `cargo test service` as baseline
- [ ] Extract `build_chunk_calculator()` method/free function (lines 424-440)
- [ ] Target: `analyze()` <= 15 complexity
- [ ] Run `cargo test service` + `cargo test analyzer_service_test` +
      `cargo fmt` + `cargo clippy`

#### 6e. NormalizeWhitespace::transform() + EmptyLineFilter::transform() in normalize.rs -- scores 16 + 16

- [ ] Run `cargo test` with normalize tests as baseline
- [ ] Extract `normalize_newlines()` free function for NormalizeWhitespace (lines 34-48)
- [ ] Extract `filter_empty_lines()` free function for EmptyLineFilter (lines 127-139)
- [ ] Target: both transforms <= 15 complexity
- [ ] Run `cargo test` + `cargo fmt` + `cargo clippy`

#### 6f. WindowedLineDeduplicator::flush_lines() in aggressive.rs -- score 16

- [ ] Run `cargo test aggressive_transform_test` as baseline
- [ ] Extract `is_line_redundant()` free function (lines 609-624)
- [ ] Target: `flush_lines()` <= 15 complexity
- [ ] Run `cargo test aggressive_transform_test` + `cargo fmt` + `cargo clippy`

#### 6g. extract_json() in backend/mod.rs -- score 16

- [ ] Run `cargo test backend` as baseline
- [ ] Extract `try_claude_wrapper()` free function (lines 344-368)
- [ ] Target: `extract_json()` <= 15 complexity
- [ ] Run `cargo test backend` + `cargo fmt` + `cargo clippy`

Files: `src/analyzer/chunk.rs`, `src/analyzer/transforms/cleaner.rs`, `src/analyzer/extractor.rs`, `src/analyzer/service.rs`, `src/analyzer/transforms/normalize.rs`, `src/analyzer/transforms/aggressive.rs`, `src/analyzer/backend/mod.rs`
Depends on: none

---

### Stage 7: clipboard + tests

Goal: Reduce complexity of the remaining 2 violations.
Sub-ADRs: [ADR-clipboard.md](ADR-clipboard.md), [ADR-tests.md](ADR-tests.md)

Owner: implementer

#### 7a. Copy::file() in clipboard/copy.rs -- score 16

- [ ] Run `cargo test` as baseline
- [ ] Extract `try_copy_file_with_tools()` method (lines 49-70)
- [ ] Extract `try_copy_text_with_tools()` method (lines 85-104) if needed
- [ ] Target: `file()` <= 15 complexity
- [ ] Run `cargo test` + `cargo fmt` + `cargo clippy`

#### 7b. render_viewport_snapshot() in snapshot_player_test.rs -- score 21

- [ ] Run `cargo test snapshot_player_test` as baseline
- [ ] Extract `render_snapshot_row()` free function (lines 354-378)
- [ ] Target: `render_viewport_snapshot()` <= 15 complexity
- [ ] Assertions and test intent MUST remain unchanged
- [ ] Run `cargo test snapshot_player_test` + `cargo fmt` + `cargo clippy`

Files: `src/clipboard/copy.rs`, `tests/integration/snapshot_player_test.rs`
Depends on: none

---

### Stage 8: Discovery -- unlisted Medium-tier violations

Goal: Discover and fix the remaining ~7 violations not explicitly listed in REQUIREMENTS.

Owner: implementer

- [ ] Run SonarCloud analysis on the branch (or check the SonarCloud dashboard) to
      identify all remaining violations at >= 15 not listed in REQUIREMENTS.
- [ ] For each discovered violation, apply the same TDD cycle: verify baseline, extract
      helper(s), verify regression, format and lint.
- [ ] Update this PLAN with the discovered functions and their extraction patterns.
- [ ] Target: all discovered functions <= 15 complexity

Files: to be determined
Depends on: none

---

## Dependencies

What must be done before what:

- **Within Stage 1**: Sub-stages 1a through 1e are sequential (same file: `list_app.rs`).
  Work bottom-to-top to minimize line-number drift.
- **Within Stage 3**: Sub-stages 3b and 3c are sequential (same file: `config.rs`,
  3b at line 243 first, 3c at line 70 second). Stage 3a is independent.
- **All other stages are independent of each other** (no overlapping files between stages).
- **Within each multi-file stage**: All sub-stages in different files can run in parallel.

Parallelization summary:
```
Stage 1: [1a -> 1b -> 1c -> 1d -> 1e]      (sequential, one file)
Stage 2: [2a | 2b | 2c]                      (all parallel, different files)
Stage 3: [3a | (3b -> 3c)]                   (3a parallel with 3b/3c; 3b before 3c)
Stage 4: [4a]                                 (single file)
Stage 5: [5a | 5b]                            (parallel, different files)
Stage 6: [6a | 6b | 6c | 6d | 6e | 6f | 6g] (all parallel, different files)
Stage 7: [7a | 7b]                            (parallel, different files)
Stage 8: [8]                                  (discovery task)
```

All eight stages can execute in parallel since no files overlap across stages.

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1a | pending | list_app.rs draw() -- score 80 |
| 1b | pending | list_app.rs handle_mouse() -- score 48 |
| 1c | pending | list_app.rs handle_rename_input_key() -- score 18 |
| 1d | pending | list_app.rs render_context_menu_modal() -- score 18 |
| 1e | pending | list_app.rs handle_normal_key() -- score 17 |
| 2a | pending | file_explorer.rs render() -- score 79 |
| 2b | pending | event_bus.rs new() -- score 43 |
| 2c | pending | cleanup_app.rs select_by_glob() -- score 16 |
| 3a | pending | analyze.rs handle() -- score 62 |
| 3b | pending | config.rs print_diff_preview() -- score 28 |
| 3c | pending | config.rs handle_migrate() -- score 17 |
| 4a | pending | viewport.rs render_viewport() + render_single_line() -- scores 44 + 27 |
| 5a | pending | migrate/mod.rs sort_toml_text() -- score 34 |
| 5b | pending | docs.rs insert_optional_field_templates() -- score 28 |
| 6a | pending | chunk.rs find_segments_for_range() -- score 27 |
| 6b | pending | cleaner.rs ContentCleaner -- score 22 |
| 6c | pending | extractor.rs redistribute_time() -- score 20 |
| 6d | pending | service.rs analyze() -- score 18 |
| 6e | pending | normalize.rs NormalizeWhitespace + EmptyLineFilter -- scores 16 + 16 |
| 6f | pending | aggressive.rs flush_lines() -- score 16 |
| 6g | pending | backend/mod.rs extract_json() -- score 16 |
| 7a | pending | copy.rs file() -- score 16 |
| 7b | pending | snapshot_player_test.rs render_viewport_snapshot() -- score 21 |
| 8 | pending | Discovery: unlisted Medium-tier violations (up to 7 functions) |
