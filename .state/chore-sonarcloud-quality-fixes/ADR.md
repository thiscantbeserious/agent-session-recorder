# ADR: Reduce Cognitive Complexity for SonarCloud Quality Gate

## Status
Accepted

## Context

SonarCloud reports 126 cognitive complexity violations (rule `rust:S3776`, threshold 15).
The worst offenders score 28-50, concentrated in analyzer transforms and TUI draw/input
handlers. These high-complexity functions contain deeply nested match arms, inline loop
bodies, and interleaved concerns (event processing + rendering, state transitions +
output building). The README also lacks SonarCloud badges, making project health
invisible to contributors.

The refactoring scope is limited to functions scoring >= 20 (approximately 10 functions
across 7 files). Functions scoring 15-19 are explicitly deferred.

### Forces

- **SonarCloud quality gate**: PR must pass with no new code smells above threshold
- **No behavioral changes**: This is pure structural refactoring -- extract helpers only
- **Test coverage gap**: TUI files (player, cleanup_app, list_app) rely heavily on
  integration/manual testing; unit tests exist but do not exercise draw()/handle_mouse()
  directly. Refactoring these requires caution.
- **File size constraint**: Several files already exceed the 400-line project guideline
  (service.rs at 1057 lines, list_app.rs at 1850 lines, aggressive.rs at 655 lines).
  Helper extraction should not worsen this; where possible it should improve it.

## Options Considered

### Option 1: Extract helper functions within same file (inline refactoring)

Extract the body of high-complexity match arms and loop bodies into private helper
methods on the same struct/impl block. No module restructuring.

- Pros: Minimal diff, low risk, no import changes, easy to review, directly targets
  SonarCloud metric
- Cons: Large files stay large (does not address file-size violations), extracted
  helpers share mutable state via `&mut self`

### Option 2: Extract helpers + split oversize files into submodules

Same as Option 1, but also split files exceeding 400 lines into submodules (e.g.,
`service.rs` -> `service/mod.rs` + `service/reporting.rs`).

- Pros: Addresses both complexity and file-size guidelines, better long-term
  maintainability
- Cons: Larger diff, higher risk of merge conflicts, more files to review, increases
  scope beyond the SonarCloud-only objective

### Option 3: Restructure using trait-based dispatch patterns

Replace deeply nested match arms with trait objects or enum dispatch to flatten
control flow entirely.

- Pros: Eliminates complexity structurally, more idiomatic for some patterns
- Cons: Major architectural change, high risk of behavioral changes, far exceeds
  scope of a "fix code smells" chore

## Decision

**Option 1: Extract helper functions within same file.**

Rationale: The requirements explicitly state "pure refactoring only: extract helper
functions, do not change logic or data flow." Option 1 directly satisfies this with
minimal risk. File-size improvements can be addressed in a separate follow-up chore
if desired. Option 3 is out of scope.

Each high-complexity function will be decomposed by identifying cohesive blocks of
logic (typically 5-15 lines) that serve a single purpose, then extracting those
blocks into private helper functions with descriptive names. The parent function
becomes a coordinator that calls helpers in sequence.

### Extraction Strategy by Pattern

1. **Transform `transform()` methods** (terminal.rs, dedupe.rs, aggressive.rs, cleaner.rs):
   These iterate over events with a drain loop. Extract the match-arm bodies into
   `handle_output_event()` / `handle_non_output_event()` style helpers. For nested
   char-by-char loops (dedupe.rs), extract the inner character processing.

2. **TUI `draw()` methods** (cleanup_app.rs, list_app.rs):
   These build UI with large match blocks for mode-dependent rendering. Extract
   `render_status_for_mode()` and `footer_text_for_mode()` helpers that return
   the mode-specific content.

3. **TUI `handle_mouse()` methods** (cleanup_app.rs, list_app.rs):
   Nested match on mode then mouse event kind. Extract per-mode handlers:
   `handle_normal_mouse_click()`, etc.

4. **Orchestration methods** (service.rs `analyze()`, player `run_main_loop()`):
   Long sequential pipelines with branches. Extract reporting, debug output,
   and playback processing into focused helpers.

## Consequences

- What becomes easier:
  - SonarCloud quality gate passes
  - Individual transform steps are testable in isolation (future test improvement)
  - Functions become scannable -- coordinator pattern shows the "what", helpers show
    the "how"
  - Onboarding developers can follow the flow more easily

- What becomes harder:
  - Navigating code requires following one more level of indirection
  - File line counts remain high for some files (deferred concern)

- Follow-ups to scope for later:
  - Split oversize files (service.rs, list_app.rs, aggressive.rs) into submodules
  - Address remaining 116 complexity violations (scores 15-19)
  - Add unit tests for extracted TUI helpers where feasible

## Decision History

1. Scope limited to functions with complexity >= 20 per requirements; 15-19 deferred.
2. Chose inline extraction (Option 1) over module restructuring (Option 2) to minimize
   risk and match the "pure refactoring" constraint.
3. TUI-critical files (player/mod.rs, cleanup_app.rs) need existing test verification
   before refactoring; list_app.rs also flagged for draw() and handle_mouse() if those
   exceed complexity 20.
4. README badge addition is independent work that can proceed in parallel with any
   refactoring stage.
