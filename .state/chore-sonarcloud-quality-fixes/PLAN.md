# Plan: Reduce Cognitive Complexity for SonarCloud Quality Gate

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. The `TerminalTransform::transform()` method (complexity 50) mutates multiple struct
   fields (`stable_lines_count`, `last_cursor_pos`, `row_write_counts`) within the same
   loop iteration. Extracted helpers must take `&mut self` carefully to avoid borrow
   conflicts. Determine whether helper signatures need explicit field parameters or if
   `&mut self` works throughout.

2. The `ContentCleaner::clean()` method uses a large match on `(state, char)` tuples.
   Splitting this match across multiple functions may reduce complexity score but could
   hurt readability if the state machine is fragmented. Determine the right granularity:
   group by ANSI state (escape vs. CSI vs. OSC) rather than splitting every arm.

3. `list_app.rs` `draw()` and `handle_mouse()` likely exceed complexity 20 (the file was
   not in the original SonarCloud list, but it shares identical patterns with
   `cleanup_app.rs` and is 1850 lines). Confirm via `cargo clippy` or SonarCloud
   whether these are flagged; if yes, include them in Stage 3.

## TDD Cycle for Pure Refactoring

Since this is pure refactoring (no new behavior), each function follows this cycle:

1. **Red/Green (test baseline)**: Identify or write behavior-preserving tests for the
   function being refactored. Tests must live in `tests/` directory per project
   convention, not in inline `#[cfg(test)]` modules. Run tests -- they must pass
   before touching any source code.
2. **Refactor**: Extract helper functions to reduce complexity. No behavioral changes.
3. **Green (regression check)**: Run the same tests again -- they must still pass.
4. **Format and lint**: `cargo fmt` and `cargo clippy`.

## Stages

### Stage 1: Analyzer transforms -- terminal.rs, dedupe.rs, cleaner.rs

Goal: Reduce complexity of the three highest-scoring analyzer transforms.

Owner: implementer

#### 1a. TerminalTransform::transform() (complexity 50)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: `tests/integration/analyzer_content_test.rs` tests
      the full pipeline but does not isolate `TerminalTransform`. No dedicated test file
      exists in `tests/`.
- [x] Create `tests/integration/terminal_transform_test.rs` with behavior-preserving
      tests:
  - Test basic output processing (single event in, stable lines out)
  - Test scroll handling (events that cause scrolled lines are emitted)
  - Test noise filtering (behaviorally noisy rows are excluded)
  - Test final flush (remaining buffer content is emitted at end)
  - Test time accumulation (collapsed events carry time forward)
  - Test resize event passthrough
  - Test hash deduplication (identical redrawn lines are deduplicated)
- [x] Run `cargo test terminal_transform_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `handle_output_event()` for the `EventType::Output` match arm body
- [x] Extract `emit_scrolled_lines()` for the scroll handling block (lines 148-173)
- [x] Extract `emit_stable_lines()` for the stable-line emission block (lines 186-226)
- [x] Extract `flush_remaining_lines()` for the final flush block (lines 251-272)
- [x] Target: each extracted function <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test terminal_transform_test` -- all tests must still pass
- [x] Run `cargo test analyzer_content_test` -- full pipeline tests must still pass
- [x] `cargo fmt` and `cargo clippy`

#### 1b. DeduplicateProgressLines::transform() (complexity 43)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: `src/analyzer/transforms/dedupe.rs` has 3 inline
      tests; `tests/integration/analyzer_content_test.rs` has pipeline and marker
      preservation tests. No dedicated `tests/` file.
- [x] Create `tests/integration/dedupe_transform_test.rs` with behavior-preserving
      tests that replicate and extend the inline tests:
  - Test CR-line collapsing (spinner sequences reduced to final line)
  - Test relative timestamp preservation (total time is preserved)
  - Test time carry-through from collapsed progress events
  - Test non-output event passthrough (markers flush pending state)
  - Test CR-then-LF sequence (treated as line break, not overwrite)
  - Test trailing content without newline (flushed at end)
- [x] Run `cargo test dedupe_transform_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `flush_non_output()` for non-output event handling (lines 43-63)
- [x] Extract `process_char()` for the per-character state machine (lines 68-105)
- [x] Extract `emit_line_with_newline()` for the repeated flush pattern (lines 72-76, 93-96)
- [x] Target: each extracted function <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test dedupe_transform_test` -- all tests must still pass
- [x] Run `cargo test analyzer_content_test` -- pipeline tests must still pass
- [x] `cargo fmt` and `cargo clippy`

#### 1c. ContentCleaner::clean() (complexity 22)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: `src/analyzer/transforms/cleaner.rs` has 10 inline
      tests; `tests/integration/analyzer_content_test.rs` has snapshot and property tests.
      Good coverage exists but is split between inline and integration.
- [x] Create `tests/integration/content_cleaner_test.rs` with behavior-preserving tests
      that replicate the inline tests:
  - Test CSI color code stripping
  - Test cursor movement stripping
  - Test OSC sequence stripping (BEL and ST terminated)
  - Test control character stripping
  - Test preservation of tab, newline, carriage return
  - Test preservation of semantic characters (checkmark, warning, etc.)
  - Test box drawing character stripping
  - Test spinner character stripping
  - Test progress block stripping
  - Test nested/partial escape sequences
- [x] Run `cargo test content_cleaner_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `handle_escape_char(&mut self, c: char)` for all non-Normal ANSI parse
      states (group Escape/Csi/CsiParams/Osc/OscEscape arms)
- [x] The existing `process_normal_char()` is already well-extracted; no changes needed
- [x] Target: `clean()` <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test content_cleaner_test` -- all tests must still pass
- [x] Run `cargo test analyzer_content_test` -- pipeline tests must still pass
- [x] `cargo fmt` and `cargo clippy`

Files: `src/analyzer/transforms/terminal.rs`, `src/analyzer/transforms/dedupe.rs`, `src/analyzer/transforms/cleaner.rs`, `tests/integration/terminal_transform_test.rs` (new), `tests/integration/dedupe_transform_test.rs` (new), `tests/integration/content_cleaner_test.rs` (new)
Depends on: none

Considerations:
- Edge case: `terminal.rs` helpers mutate struct fields -- ensure no double-borrow of `&mut self` when calling helpers within the drain loop. If needed, pass individual fields instead of `&mut self`.
- Watch out for: Time accumulation logic in dedupe.rs is subtle (accumulated_time must carry through collapsed events). Each extracted function must preserve the same accumulation behavior.
- The `cleaner.rs` state machine match is intentionally flat; only extract the escape-state arms (not the Normal arm) to keep the top-level structure readable.
- New test files must be registered in `tests/integration.rs` as `mod` entries.

### Stage 2: Analyzer transforms -- aggressive.rs, service.rs

Goal: Reduce complexity of aggressive transforms and the analyzer service orchestration.

Owner: implementer

#### 2a. GlobalDeduplicator::transform() (complexity 32)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: No dedicated tests for `GlobalDeduplicator` in
      `tests/` or inline. Only exercised indirectly through the full extraction pipeline.
- [x] Create `tests/integration/aggressive_transform_test.rs` with behavior-preserving
      tests:
  - Test windowed event hash deduplication (large duplicate events are removed)
  - Test small events bypass hash check (below `min_hash_bytes` threshold)
  - Test line frequency capping (lines repeated beyond max are removed)
  - Test empty/whitespace lines are always kept
  - Test non-output event passthrough
  - Test time accumulation for deduplicated events
  - Test hash window eviction (old hashes are removed when window overflows)
- [x] Run `cargo test aggressive_transform_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `check_windowed_hash(&mut self, event: &Event) -> bool` for windowed
      event hashing (lines 364-378) -- returns true if event is a duplicate
- [x] Extract `apply_line_frequency_cap(&mut self, data: &str) -> String` for line
      frequency capping (lines 382-403)
- [x] Target: `transform()` <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test aggressive_transform_test` -- all tests must still pass
- [x] `cargo fmt` and `cargo clippy`

#### 2b. SimilarityFilter::transform() (complexity 29)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: No dedicated tests for `SimilarityFilter::transform()`
      in `tests/`. The `calculate_similarity` method has good unit-level coverage inline.
- [x] Add behavior-preserving tests to `tests/integration/aggressive_transform_test.rs`:
  - Test similar consecutive lines are collapsed with count message
  - Test dissimilar lines are kept
  - Test short lines (< 30 chars) are never collapsed
  - Test non-output events flush pending skips
  - Test time accumulation through collapsed lines
  - Test flush at end of event stream
- [x] Run `cargo test aggressive_transform_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `process_output_lines()` for line-by-line similarity processing
      (lines 111-146)
- [x] Target: `transform()` <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test aggressive_transform_test` -- all tests must still pass
- [x] `cargo fmt` and `cargo clippy`

#### 2c. AnalyzerService::analyze() (complexity 28)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: `src/analyzer/service.rs` has extensive inline tests
      (mock backend, file integrity, error paths). These cover the function well.
- [x] Create `tests/integration/analyzer_service_test.rs` with behavior-preserving tests
      that replicate key inline tests:
  - Test successful analysis with mock backend returns markers
  - Test empty content returns NoContent error
  - Test file not found returns IoError
  - Test existing markers are detected and reported
  - Test sequential mode forces worker count to 1
  - Test debug output mode writes file and returns early
- [x] Run `cargo test analyzer_service_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `print_extraction_stats()` as a free function for stats printing
      (lines 268-323)
- [x] Extract `handle_debug_output()` for the debug output block (lines 328-352)
- [x] Extract `report_analysis_summary()` for the summary reporting block (lines 438-468)
- [x] Target: `analyze()` <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test analyzer_service_test` -- all tests must still pass
- [x] Run existing inline `cargo test service::tests` -- must still pass
- [x] `cargo fmt` and `cargo clippy`

Files: `src/analyzer/transforms/aggressive.rs`, `src/analyzer/service.rs`, `tests/integration/aggressive_transform_test.rs` (new), `tests/integration/analyzer_service_test.rs` (new)
Depends on: none

Considerations:
- Edge case: `GlobalDeduplicator::check_windowed_hash()` must return whether to skip the event, while also maintaining the hash window. Be careful with the control flow: if it returns "duplicate", the caller adds time and continues.
- Watch out for: `service.rs` `analyze()` has two early-return paths (debug output, NoContent). Extracted helpers must not change when those returns fire.
- The `SimilarityFilter::transform()` has a `flush_skips()` call inside the line loop AND after it; ensure the extracted helper preserves both flush points.
- New test files must be registered in `tests/integration.rs` as `mod` entries.

### Stage 3: TUI files -- cleanup_app.rs, player/mod.rs

Goal: Reduce complexity in TUI rendering and input handling.

Owner: implementer

#### 3a. run_main_loop() in player/mod.rs (complexity 42)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: `tests/integration/snapshot_player_test.rs` has
      comprehensive snapshot tests for all rendering components (progress bar, status
      bar, viewport, help overlay, scroll indicators, full frames). `src/player/mod.rs`
      has 5 inline tests for `PlaybackResult`. `run_main_loop()` itself is not directly
      testable (requires real terminal + event loop) but its extracted sub-functions
      will be testable.
- [x] Verify snapshot baseline: run `cargo test snapshot_player_test` -- all snapshot
      tests must pass. These serve as the regression safety net.
- [x] Create `tests/integration/player_loop_test.rs` with behavior-preserving tests
      for the extractable sub-functions (test after extraction, but design now):
  - Test `advance_playback()` processes events up to elapsed time
  - Test `advance_playback()` respects pause state (no-op when paused)
  - Test `advance_playback()` handles resize events
  - Test `advance_playback()` caps elapsed time at total duration
- [x] Run `cargo test player_loop_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `process_pending_events()` for the input event polling loop (lines 170-195)
- [x] Extract `advance_playback()` for the event processing block (lines 198-224)
- [x] Extract `render_frame()` for the full rendering block (lines 233-322)
- [x] Target: `run_main_loop()` <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test player_loop_test` -- all tests must still pass
- [x] Run `cargo test snapshot_player_test` -- all snapshots must still match
- [x] `cargo fmt` and `cargo clippy`

#### 3b. CleanupApp::draw() (complexity ~43)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: `src/tui/cleanup_app.rs` has 10 inline tests for
      Mode and glob matching. `tests/integration/snapshot_tui_test.rs` has visual
      snapshot tests for shared TUI components. `draw()` itself is not directly testable
      (requires `App::draw()` which needs a terminal) but the extracted status/footer
      helpers will be pure functions that are testable.
- [x] Verify snapshot baseline: run `cargo test snapshot_tui_test` -- all snapshot tests
      must pass.
- [x] Create `tests/integration/cleanup_draw_test.rs` with behavior-preserving tests
      for the extractable pure functions (test after extraction, but design now):
  - Test `footer_text_for_mode()` returns correct text for each Mode variant
  - Test `footer_text_for_mode()` returns selection-aware text in Normal mode
  - Test status text generation for Normal mode with no selection
  - Test status text generation for Normal mode with active filters
  - Test status text generation for Normal mode with selection count
- [x] Run `cargo test cleanup_draw_test` -- all tests must pass (green baseline)

**Refactor -- extract helpers:**
- [x] Extract `status_line_content()` for mode-dependent status text (lines 564-629)
- [x] Extract `footer_text_for_mode()` for footer keybinding text (lines 633-647)
- [x] Target: `draw()` <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test cleanup_draw_test` -- all tests must still pass
- [x] Run `cargo test snapshot_tui_test` -- all snapshots must still match
- [x] `cargo fmt` and `cargo clippy`

#### 3c. CleanupApp::handle_mouse() (complexity ~25)

**Red/Green -- establish test baseline:**
- [x] Audit existing test coverage: No direct tests for `handle_mouse()`. The function
      dispatches based on mode and mouse event kind. Cannot be tested without a
      real terminal, but the extracted click handler can be designed as a pure-ish
      function.
- [x] Verify the existing `cargo test cleanup_app` inline tests pass as baseline.
- [x] Note: `handle_mouse()` is tightly coupled to `App::size()` and `SharedState`.
      Extracted `handle_normal_mouse_click()` takes `&mut SharedState`, `height`, and
      `click_row` as explicit parameters. Mechanical extraction -- accepted gap as noted
      in PLAN considerations.

**Refactor -- extract helpers:**
- [x] Extract `handle_normal_mouse_click()` for Normal mode left-click handling
      (lines 445-463)
- [x] Target: `handle_mouse()` <= 15 complexity

**Green -- verify no regression:**
- [x] Run `cargo test` (full suite) -- no regressions
- [x] `cargo fmt` and `cargo clippy`

Files: `src/player/mod.rs`, `src/tui/cleanup_app.rs`, `tests/integration/player_loop_test.rs` (new), `tests/integration/cleanup_draw_test.rs` (new)
Depends on: none

Considerations:
- Edge case: `run_main_loop()` has a `continue` after partial render (line 264) that skips the sleep at loop end. The extracted `render_frame()` must signal whether to skip the sleep (via return value or flag).
- Watch out for: `draw()` captures many fields in a closure for `self.app.draw(|frame| {...})`. Extracted helpers must work with the already-borrowed fields, not require `&self` which conflicts with the mutable `self.app` borrow. Design them as free functions or static methods taking explicit parameters.
- TUI `handle_mouse()` is difficult to unit-test. If the extracted helper cannot be tested in isolation, accept this gap and rely on the mechanical nature of the extraction plus the full `cargo test` suite.
- `list_app.rs` has the same patterns but is not in the original SonarCloud list. Defer to a follow-up.
- New test files must be registered in `tests/integration.rs` as `mod` entries.

### Stage 4: README badges

Goal: Add SonarCloud quality badges to the README.

Owner: implementer

This stage has no TDD cycle -- it is a pure documentation change with no testable behavior.

- [x] Determine the correct SonarCloud project key (check `.sonarcloud.properties` or SonarCloud dashboard)
- [x] Add three SonarCloud badges to README.md after the existing badge row (line 17-22):
  - Quality Gate Status badge
  - Maintainability Rating badge
  - Code Smells count badge
- [x] Use the standard SonarCloud badge markdown format:
  ```
  [![Quality Gate](https://sonarcloud.io/api/project_badges/measure?project=<KEY>&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=<KEY>)
  [![Maintainability](https://sonarcloud.io/api/project_badges/measure?project=<KEY>&metric=sqale_rating)](https://sonarcloud.io/summary/new_code?id=<KEY>)
  [![Code Smells](https://sonarcloud.io/api/project_badges/measure?project=<KEY>&metric=code_smells)](https://sonarcloud.io/summary/new_code?id=<KEY>)
  ```
- [x] Verify badges render correctly in markdown preview

Files: `README.md`
Depends on: none

Considerations:
- The project key format is typically `<org>_<repo>` for GitHub-hosted projects. Check the SonarCloud configuration in the repo.
- Place badges on the same line as existing badges for visual consistency.

## Dependencies

What must be done before what:

- Stage 1 and Stage 2 are independent (no file overlap) and can run in parallel.
- Stage 3 is independent of Stage 1 and Stage 2 (no file overlap) and can run in parallel.
- Stage 4 is fully independent and can run in parallel with any other stage.
- All four stages can theoretically execute in parallel since no files overlap.
- Within each stage, sub-stages (a, b, c) are sequential: each function's TDD cycle
  must complete before moving to the next function in the same stage.

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1a | complete | terminal.rs -- tests + refactor |
| 1b | complete | dedupe.rs -- tests + refactor |
| 1c | complete | cleaner.rs -- tests + refactor |
| 2a | done | aggressive.rs GlobalDeduplicator -- tests + refactor |
| 2b | done | aggressive.rs SimilarityFilter -- tests + refactor |
| 2c | done | service.rs -- tests + refactor |
| 3a | complete | player/mod.rs -- advance_playback + render_frame + process_pending_events extracted |
| 3b | complete | cleanup_app.rs draw() -- status_line_content + footer_text_for_mode extracted |
| 3c | complete | cleanup_app.rs handle_mouse() -- handle_normal_mouse_click extracted |
| 4 | complete | README badges -- Quality Gate, Maintainability, Code Smells |
