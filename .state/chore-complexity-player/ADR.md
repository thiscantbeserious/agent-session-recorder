# ADR: Reduce Cognitive Complexity for SonarCloud Quality Gate -- Round 2

## Status
Proposed

## Context

After PR #141 eliminated all cognitive complexity >= 20 violations, SonarCloud still
reports 32 remaining violations of rule `rust:S3776` (threshold 15). The worst offenders
are TUI input/draw handlers (scores 80, 79, 48) and command handlers (score 62). The
SonarCloud quality gate remains blocked by this technical debt.

PR #141's ADR (`.state/chore-sonarcloud-quality-fixes/ADR.md`) established a proven
approach: **inline helper-function extraction within the same file/impl block** (Option 1).
That approach successfully resolved all >= 20 violations with zero regressions. This ADR
reuses the same approach for the remaining 32 violations.

### Forces

- **Proven pattern**: PR #141 demonstrated that inline extraction works, reviews cleanly,
  and introduces no regressions. No reason to deviate.
- **Scale**: 32 functions across 14+ files is roughly 3x the scope of PR #141 (10 functions
  across 7 files). Parallelization by file is essential.
- **TUI file concentration**: `src/tui/list_app.rs` alone has 5 violations (lines 213,
  704, 1047, 1254, 1426). These must be planned as a single unit to avoid conflicting edits.
- **Test file refactoring**: One violation is in a test file
  (`tests/integration/snapshot_player_test.rs`). Extraction must not alter assertions or
  test intent.
- **Many functions are `#[cfg(not(tarpaulin_include))]`**: Several command handlers
  (`analyze::handle`, `config::handle_migrate`, `config::print_diff_preview`) are excluded
  from code coverage. These cannot be meaningfully unit-tested; the TDD "RED" phase for
  these means verifying `cargo test` passes as baseline, not writing new tests.
- **Incomplete violation list**: REQUIREMENTS explicitly names 25 of 32 violations. The
  remaining 7 Medium-tier violations are expected in `src/main.rs` and other files. The
  implementer must discover these via `cargo clippy` or the SonarCloud dashboard and include
  them in the work.

## Options Considered

### Option 1: Inline helper extraction (same approach as PR #141)

Extract cohesive blocks into private helper functions within the same `impl` block or
module. No module restructuring.

- Pros: Proven pattern, minimal diff per function, low risk, directly targets the
  SonarCloud metric, easy to review
- Cons: Large files remain large (deferred concern), 32 functions means a larger overall
  diff

### Option 2: Inline extraction + module splits for oversize files

Same as Option 1, but also split `list_app.rs` (1700+ lines) into submodules during the
refactoring.

- Pros: Addresses both complexity and file-size concerns simultaneously
- Cons: Larger scope, higher risk of merge conflicts, mixes two objectives (complexity
  reduction and file organization), harder to review

## Decision

**Option 1: Inline helper extraction**, consistent with PR #141.

Rationale: The requirements explicitly state "pure refactoring only." Mixing in module
restructuring would increase risk and review burden without being required by the SonarCloud
quality gate. File-size improvements can follow in a separate chore.

## Module-Scoped Sub-ADRs

This ADR is decomposed into per-module sub-ADRs, each containing source-verified function
analysis, extraction targets with line ranges, borrow checker constraints, and testability
assessment. The sub-ADRs are:

| Sub-ADR | Module | Files | Violations | Total Score |
|---------|--------|-------|------------|-------------|
| [ADR-tui-list-app.md](ADR-tui-list-app.md) | tui/list_app | `src/tui/list_app.rs` | 5 | 181 |
| [ADR-tui-widgets.md](ADR-tui-widgets.md) | tui/widgets | `src/tui/widgets/file_explorer.rs` | 1 | 79 |
| [ADR-tui-event-bus.md](ADR-tui-event-bus.md) | tui/event_bus | `src/tui/event_bus.rs` | 1 | 43 |
| [ADR-tui-cleanup-app.md](ADR-tui-cleanup-app.md) | tui/cleanup_app | `src/tui/cleanup_app.rs` | 1 | 16 |
| [ADR-commands.md](ADR-commands.md) | commands | `src/commands/analyze.rs`, `src/commands/config.rs` | 3 | 107 |
| [ADR-analyzer.md](ADR-analyzer.md) | analyzer | 7 files under `src/analyzer/` | 9 | 167 |
| [ADR-player.md](ADR-player.md) | player | `src/player/render/viewport.rs` | 2 | 71 |
| [ADR-config.md](ADR-config.md) | config | `src/config/migrate/mod.rs`, `src/config/docs.rs` | 2 | 62 |
| [ADR-clipboard.md](ADR-clipboard.md) | clipboard | `src/clipboard/copy.rs` | 1 | 16 |
| [ADR-tests.md](ADR-tests.md) | tests | `tests/integration/snapshot_player_test.rs` | 1 | 21 |

### Common TDD Cycle for Pure Refactoring

Since this is pure refactoring (no new behavior), every function follows this cycle:

1. **RED/GREEN (test baseline)**: Verify `cargo test` passes before touching source code.
   For functions with existing dedicated tests, run those specifically. For functions without
   tests (TUI handlers, command handlers), the full test suite is the baseline.
2. **REFACTOR**: Extract helper functions to reduce complexity below 15. No behavioral changes.
3. **GREEN (regression check)**: Run the same tests again -- they must still pass.
4. **Format and lint**: `cargo fmt` and `cargo clippy`.

### Common Extraction Patterns

- **Free functions**: Required when `self` is already mutably borrowed (e.g., `draw()` closures,
  `render()` trait implementations). Take explicit parameters instead of `&self`.
- **Methods on `&mut self`**: Used when the function already has `&mut self` and no conflicting
  borrows exist (e.g., `handle_mouse()` delegates to `handle_normal_mouse(&mut self, ...)`).
- **Static methods / associated functions**: Used for pure computation that does not need any
  instance state.

## Consequences

- What becomes easier:
  - SonarCloud quality gate passes
  - Each extracted helper is a named, scannable unit of work
  - Future unit tests can target individual helpers
  - Onboarding developers can follow coordinator-pattern functions

- What becomes harder:
  - One more level of indirection when reading code
  - File line counts remain high for some files (deferred)

- Follow-ups to scope for later:
  - Split oversize files (`list_app.rs`, `aggressive.rs`) into submodules
  - Add unit tests for extracted TUI helpers where feasible

## Decision History

1. Reuse Option 1 (inline extraction) from PR #141 ADR -- proven approach, same constraints.
2. `list_app.rs` has 5 violations -- plan all 5 together as one unit to avoid conflicting
   edits across separate passes.
3. Functions marked `#[cfg(not(tarpaulin_include))]` cannot have meaningful unit tests
   written for the RED phase; baseline `cargo test` pass is the TDD gate for those.
4. Test file `snapshot_player_test.rs` extraction must only consolidate setup/repeated
   control flow, not alter assertions or test intent.
5. 7 of 32 Medium-tier violations are not explicitly listed in REQUIREMENTS. The implementer
   must discover and fix these as part of the work.
6. Restructured monolithic ADR into per-module sub-ADRs for thorough source-level analysis
   of each extraction target.
