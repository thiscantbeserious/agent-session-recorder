# Requirements: Consolidate theme and branding into a top-level theme module

## Problem Statement
Visual presentation code is scattered across two unrelated locations:
- `src/tui/theme.rs` (291 lines) — Theme struct, style helpers, ANSI color conversion, CLI help colorization
- `src/branding.rs` (161 lines) — Logo assets, banner printing, box drawing, string truncation

Neither belongs where it currently lives:
- `theme.rs` is under `tui/` but serves the entire codebase (CLI commands, recording, branding) — not just TUI
- `branding.rs` sits at the crate root despite being a presentation concern

Both deal with "how things look" and should be consolidated into a single top-level `theme` module with a clean internal split by concern.

## Desired Outcome
A new top-level `src/theme/` module that owns all visual presentation, split by concern:
- `mod.rs` — Theme struct, color palette, presets, `current_theme()`, re-exports
- `logo.rs` — Logo assets, banner printing, box drawing, string truncation
- `tui.rs` — ratatui Style helpers (the TUI-specific style methods)
- `cli.rs` — ANSI color helpers, CLI text formatting, `colorize_help`
- Additional files as the Architect sees fit

`src/tui/theme.rs` and `src/branding.rs` are both removed. The `tui` module no longer owns theme.

## Scope
### In Scope
- Create `src/theme/` module directory with logical subfiles
- Move `src/tui/theme.rs` content into the new module, split by concern
- Move `src/branding.rs` content into the new module
- Remove `pub mod theme` from `src/tui/mod.rs`
- Remove `pub mod branding` from `src/lib.rs`
- Add `pub mod theme` to `src/lib.rs`
- Update all import paths across `src/` and `tests/`
- Update `tui/mod.rs` re-exports to point to new location

### Out of Scope
- Changing any visual behavior (colors, logos, box widths)
- Adding new themes or theme switching logic
- Refactoring TUI rendering pipeline
- Moving asset files (logo .txt files)

## Acceptance Criteria
- [ ] `src/tui/theme.rs` no longer exists
- [ ] `src/branding.rs` no longer exists
- [ ] All theme/branding functionality lives under `src/theme/`
- [ ] Internal split follows single-responsibility (logo, tui styles, etc.)
- [ ] All existing tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] No file exceeds ~400 lines
- [ ] `include_str!` asset paths resolve correctly

## Constraints
- Pure refactoring — zero behavior changes
- Must not break integration tests or snapshot tests

## Context
- `branding.rs` was added early as a standalone module; theme was added later under `tui/`
- The dependency direction is one-way: `branding` → `tui::theme` (never the reverse)
- Many consumers use `tui::current_theme` and `tui::colorize_help` via re-exports in `tui/mod.rs`
- `recording.rs` is the primary consumer of branding functions
- Integration tests directly reference `agr::branding::*` and `agr::tui::theme::*`

---
**Sign-off:** Approved by user
