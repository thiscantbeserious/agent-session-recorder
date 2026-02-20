# Requirements: Fix SonarCloud Cognitive Complexity Issues and Add Quality Badges

## Problem Statement

SonarCloud reports 126 cognitive complexity violations (rule `rust:S3776`, threshold 15) across
the codebase. The worst offenders have complexity scores up to 50, making them difficult to reason
about, maintain, and safely extend. The README also lacks SonarCloud quality badges, so project
health is not immediately visible to contributors and users.

## Desired Outcome

All functions with cognitive complexity >= 20 are refactored to score below the threshold of 15.
The refactoring achieves this purely by extracting helper functions - no behavioral changes are
made. The SonarCloud quality gate passes on the PR. The README displays SonarCloud quality badges
(quality gate, maintainability rating, code smells count).

## Scope

### In Scope

- Refactor all functions with cognitive complexity >= 20 (approximately 8-10 files identified as
  hotspots by SonarCloud)
- Targeted files and known complexity scores at time of scan:
  - `src/analyzer/transforms/terminal.rs` - complexity 50
  - `src/analyzer/transforms/dedupe.rs` - complexity 43
  - `src/tui/cleanup_app.rs` - 4 functions with complexity 16-43
  - `src/player/mod.rs` - complexity 42
  - `src/analyzer/transforms/aggressive.rs` - two functions at complexity 29 and 32
  - `src/analyzer/service.rs` - complexity 28
  - `src/analyzer/transforms/cleaner.rs` - complexity 22
  - Any other functions found at complexity >= 20 during implementation
- Add SonarCloud badges to README:
  - Quality gate status badge
  - Maintainability rating badge
  - Code smells count badge

### Out of Scope

- Functions with cognitive complexity between 15 and 19 (deferred to a future pass)
- Any behavioral changes to existing logic
- New features or additional refactoring beyond complexity reduction
- Changes to tests beyond what is needed to keep them passing after extractions

## Acceptance Criteria

- [ ] All targeted functions (complexity >= 20) are brought below threshold 15 as verified by
      SonarCloud analysis on the PR
- [ ] `cargo test` passes with no regressions after all refactoring changes
- [ ] No behavioral changes introduced - only helper function extractions
- [ ] TUI-critical files (`src/player/mod.rs`, `src/tui/cleanup_app.rs`) have existing test
      coverage confirmed before refactoring begins; if coverage is insufficient, flag to user
      before proceeding
- [ ] SonarCloud quality gate passes on the PR
- [ ] README contains three SonarCloud badges: quality gate status, maintainability rating,
      code smells count

## Constraints

- Pure refactoring only: extract helper functions, do not change logic or data flow
- TUI-critical paths require extra care: verify tests before touching `src/player/mod.rs` and
  `src/tui/cleanup_app.rs`
- The 126 issues below complexity 20 are explicitly deferred - do not fix them in this branch
- Rust 2021 edition conventions must be followed for any new helper functions

## Context

- SonarCloud scan found 126 total issues, all cognitive complexity code smells (rule `rust:S3776`,
  threshold 15); no bugs or vulnerabilities reported
- Total estimated technical debt: ~435 minutes
- This branch addresses only the worst offenders (complexity >= 20); the remaining sub-20 issues
  will be handled in a separate future pass
- Branch: `chore/sonarcloud-quality-fixes`
- Project is hosted on GitHub; CI runs on every PR

---
**Sign-off:** Approved by user
