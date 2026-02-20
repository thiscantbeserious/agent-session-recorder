# Requirements: Fix SonarCloud Cognitive Complexity Violations — Round 2

## Problem Statement

After PR #141 eliminated all cognitive complexity >= 20 violations, SonarCloud continues to report
32 remaining violations of rule `rust:S3776` (threshold 15). The worst offenders now reach scores
of 80 (TUI input handlers) and 79 (file explorer input handler). These functions are difficult to
reason about, test in isolation, and safely extend. The SonarCloud quality gate is still blocked
by this technical debt.

## Desired Outcome

All 32 remaining cognitive complexity violations are brought below the threshold of 15 through
pure helper-function extraction. No behavioral changes are introduced. The SonarCloud quality gate
passes on the PR.

## Scope

### In Scope

Refactor all 32 functions currently flagged by SonarCloud. The complete violation list, grouped by
severity tier for implementation priority:

**Critical (score >= 40) — 6 functions**
- `src/tui/list_app.rs:1426`        `handle_input()`       score 80
- `src/tui/widgets/file_explorer.rs:958`  `handle_input()`  score 79
- `src/commands/analyze.rs:34`      `execute()`            score 62
- `src/tui/list_app.rs:1254`        `handle_key()`         score 48
- `src/player/render/viewport.rs:25`  `compute()`          score 44
- `src/tui/event_bus.rs:45`         `recv()`               score 43

**High (score 20–39) — 8 functions**
- `src/config/migrate/mod.rs:147`   `migrate_config()`     score 34
- `src/config/docs.rs:165`          `format_field()`       score 28
- `src/commands/config.rs:243`      `handle()`             score 28
- `src/analyzer/chunk.rs:285`       `create_chunks()`      score 27
- `src/player/render/viewport.rs:128`  `render_diff()`     score 27
- `src/analyzer/transforms/cleaner.rs:53`  `clean()`       score 22
- `tests/integration/snapshot_player_test.rs:327`  `test_playback()`  score 21
- `src/analyzer/extractor.rs:176`   `extract()`            score 20

**Medium (score 15–19) — 18 functions** (previously deferred, now in scope)
- `src/tui/list_app.rs:704`         `handle_scroll()`      score 18
- `src/analyzer/service.rs:378`     `analyze()`            score 18
- `src/tui/list_app.rs:1047`        `process_event()`      score 18
- `src/commands/config.rs:70`       `run()`                score 17
- `src/tui/list_app.rs:213`         `new()`                score 17
- `src/analyzer/transforms/normalize.rs:115`  `normalize()`  score 16
- `src/analyzer/transforms/aggressive.rs:579`  `transform()` score 16
- `src/clipboard/copy.rs:40`        `copy_output()`        score 16
- `src/tui/cleanup_app.rs:190`      `handle_input()`       score 16
- `src/analyzer/backend/mod.rs:338`  `process()`           score 16
- `src/analyzer/transforms/normalize.rs:31`  `normalize()`  score 16
- Any additional violations at >= 15 discovered in `src/main.rs` and remaining files reported by
  SonarCloud but not listed above

### Out of Scope

- Any functions scoring below 15 not currently flagged by SonarCloud
- Behavioral changes to existing logic
- New features or tests beyond what is needed to confirm refactoring correctness
- Architecture redesign of any module
- Changes to SonarCloud configuration or badge updates (already done in PR #141)

## Acceptance Criteria

- [ ] All 32 flagged functions (and any additional violations in `main.rs` / unlisted files) score
      below 15 as verified by SonarCloud analysis on the PR
- [ ] `cargo test` passes with zero regressions after every refactoring batch
- [ ] No behavioral changes introduced — only helper function extractions (pure refactoring)
- [ ] TUI-critical files (`src/tui/list_app.rs`, `src/tui/widgets/file_explorer.rs`,
      `src/tui/event_bus.rs`) have their existing test coverage confirmed before refactoring
      begins; if coverage is insufficient the Implementer flags this to the user before proceeding
- [ ] The integration test function `test_playback()` in
      `tests/integration/snapshot_player_test.rs:327` is refactored using only private helper
      functions within the test module — no test logic is altered
- [ ] SonarCloud quality gate passes on the PR

## Constraints

- **Pure refactoring only:** extract helper functions; do not change logic, data flow, or
  observable behavior
- **TDD-compatible approach:** each extraction must leave `cargo test` green; run tests after
  every file is complete, not just at the end
- **Inline extraction (Option 1 from PR #141 ADR):** prefer private helper functions in the same
  `impl` block or module; avoid moving code to new modules unless the extraction naturally belongs
  in an existing sibling module
- **Rust 2021 edition conventions** apply to all new helper functions
- **Large TUI files need special handling:** `src/tui/list_app.rs` has four violations at lines
  213, 704, 1047, 1254, and 1426. Treat the file as a single unit — plan all five extractions
  together to avoid conflicting edits across separate passes
- **Test file refactoring:** extracting helpers in `tests/integration/snapshot_player_test.rs`
  must not change assertions or test intent — helpers may only consolidate setup/teardown or
  repeated control flow
- The previously deferred sub-20 violations are now explicitly in scope for this round; do not
  defer them again

## Context

- PR #141 (`chore/sonarcloud-quality-fixes`) successfully fixed all >= 20 violations using the
  inline helper-extraction approach documented in its ADR; that approach is the confirmed pattern
  for this round
- The ADR from PR #141 is available at `.state/chore-sonarcloud-quality-fixes/` for reference
- SonarCloud rule: `rust:S3776`, threshold 15
- Branch for this work: `chore/sonarcloud-complexity-round-2`
- Project is hosted on GitHub; CI runs SonarCloud on every PR
- Total violations: 32 confirmed + any unlisted violations in `src/main.rs` and other files

---
**Sign-off:** Pending
