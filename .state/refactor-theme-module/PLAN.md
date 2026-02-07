# Plan: Consolidate theme and branding into a top-level theme module

References: ADR.md

## Open Questions

1. Should `tui/mod.rs` keep backward-compatible re-exports during migration?
   **Resolution:** No. Clean break -- remove re-exports and update all consumers in one pass. The old path (`tui::current_theme`) would perpetuate the wrong module ownership.

2. Should tests for `Theme` struct stay in mod.rs or move to the submodule that owns the tested methods?
   **Resolution:** Tests move with their code. Struct/preset tests stay in mod.rs, Style helper tests go to tui.rs, ANSI/CLI tests go to cli.rs.

## Stages

### Stage 1: Create `src/theme/mod.rs` with Theme struct and presets

Goal: Establish the new module with the core Theme type, presets, and `current_theme()`.

- [x] Create directory `src/theme/`
- [x] Create `src/theme/mod.rs` with:
  - Module-level doc comment
  - `pub struct Theme` (all 6 palette fields)
  - `impl Default for Theme` (delegates to `claude_code()`)
  - `impl Theme { claude_code(), classic(), ocean() }`
  - `pub fn current_theme() -> Theme`
  - Placeholder `pub mod tui;`, `pub mod cli;`, `pub mod logo;` declarations (files created in later stages)
- [x] Add `pub mod theme;` to `src/lib.rs` (alongside existing `pub mod tui;` and `pub mod branding;`)
- [x] Verify: `cargo check` passes (new module compiles, nothing depends on it yet)

Files: `src/theme/mod.rs`, `src/lib.rs`

Considerations:
- Do NOT remove `pub mod branding` or `pub mod theme` from tui/mod.rs yet -- existing code still depends on them.
- The struct fields use `ratatui::style::Color` -- this is acceptable in mod.rs since the type is just data (no widget rendering).
- Unit tests for presets (`default_theme_is_claude_code`, `classic_theme_uses_white`, `ocean_theme_uses_cyan`) move here.

### Stage 2: Create `src/theme/tui.rs` with ratatui Style helpers

Goal: Move all ratatui Style-returning methods to a dedicated TUI styles file.

- [x] Create `src/theme/tui.rs` with:
  - Module-level doc comment
  - `use ratatui::style::{Color, Modifier, Style};`
  - `use super::Theme;`
  - `impl Theme { text_style, text_secondary_style, accent_style, accent_bold_style, error_style, success_style, highlight_style }`
- [x] Move `style_helpers_return_correct_colors` and `highlight_style_uses_black_on_accent` tests here
- [x] Verify: `cargo check` passes

Files: `src/theme/tui.rs`

Considerations:
- These methods return `ratatui::style::Style` -- this is the only file in `src/theme/` that imports ratatui widget types.
- The methods are still callable on any `Theme` instance because Rust merges impl blocks.

### Stage 3: Create `src/theme/cli.rs` with ANSI helpers and colorize_help

Goal: Move all CLI text formatting to a dedicated file.

- [x] Create `src/theme/cli.rs` with:
  - Module-level doc comment
  - `pub const ANSI_RESET: &str`
  - `pub mod ansi { GRAY, GREEN, RED, DARK_GRAY, RESET }`
  - `pub fn color_to_ansi(color: Color) -> &'static str`
  - `impl Theme { accent_text, primary_text, secondary_text, error_text, success_text }` (these call `color_to_ansi`)
  - `pub fn colorize_help(text: &str) -> String` (calls `super::current_theme()`)
  - `fn find_description_start(line: &str) -> Option<usize>` (private helper)
- [x] Move `ansi_text_helpers_wrap_with_color_codes` and `color_to_ansi_maps_standard_colors` tests here
- [x] Add re-exports to `src/theme/mod.rs`:
  ```rust
  pub use cli::{colorize_help, color_to_ansi, ANSI_RESET};
  pub use cli::ansi;
  ```
- [x] Verify: `cargo check` passes

Files: `src/theme/cli.rs`, `src/theme/mod.rs`

Considerations:
- `colorize_help` calls `current_theme()` -- use `super::current_theme` since it lives in the parent mod.rs.
- The `ansi` sub-module is re-exported so `theme::ansi::GREEN` works directly.

### Stage 4: Create `src/theme/logo.rs` with branding content

Goal: Move all logo/banner/box-drawing code from `src/branding.rs` into the new theme module.

- [x] Create `src/theme/logo.rs` with:
  - Module-level doc comment
  - `pub const LOGO_FULL: &str = include_str!("../../assets/logo.txt");`
  - `pub const LOGO_START: &str = include_str!("../../assets/logo_small.txt");`
  - `pub const LOGO_DONE: &str = include_str!("../../assets/logo_small_done.txt");`
  - `pub const BOX_WIDTH: usize = 39;`
  - `pub const BOX_BOTTOM: &str = "...";`
  - All `pub fn print_*` functions (start_banner, done_banner, full_logo, box_line, box_bottom, box_prompt, box_line_end)
  - `fn colorize_recording_banner(...)` (private)
  - `pub fn truncate_str(...)`
- [x] Update internal imports: `crate::tui::theme::*` becomes `super::*` or `super::cli::*`
- [x] Add re-exports to `src/theme/mod.rs`:
  ```rust
  pub use logo::{
      print_start_banner, print_done_banner, print_full_logo,
      print_box_line, print_box_bottom, print_box_prompt, print_box_line_end,
      truncate_str, BOX_BOTTOM, BOX_WIDTH, LOGO_DONE, LOGO_FULL, LOGO_START,
  };
  ```
- [x] Verify: `cargo check` passes (logo.rs compiles, `include_str!` paths resolve)

Files: `src/theme/logo.rs`, `src/theme/mod.rs`

Considerations:
- `include_str!` paths change from `"../assets/..."` to `"../../assets/..."` because the file moves one directory deeper.
- `logo.rs` uses `current_theme()`, `color_to_ansi()`, `ANSI_RESET`, and `Theme` -- all available via `super::` imports.
- `unicode_width::UnicodeWidthStr` import moves here (only used by `truncate_str`).

### Stage 5: Migrate all consumers to new import paths

Goal: Update every file that imports from `tui::theme`, `tui::current_theme`, `tui::colorize_help`, or `branding::` to use the new `theme::` paths.

- [x] Update `src/tui/mod.rs`:
  - Remove `pub mod theme;`
  - Remove `pub use theme::{colorize_help, current_theme};`
- [x] Update `src/lib.rs`:
  - Remove `pub mod branding;`
  - Ensure `pub mod theme;` is present (added in Stage 1)
- [x] Update internal crate consumers (`super::` / `crate::` paths):
  - `src/tui/ui.rs`: `use super::current_theme` -> `use crate::theme::current_theme`
  - `src/tui/list_app.rs`: `use super::theme::current_theme` -> `use crate::theme::current_theme`
  - `src/tui/cleanup_app.rs`: `use super::theme::current_theme` -> `use crate::theme::current_theme`
  - `src/tui/widgets/logo.rs`: `use crate::tui::current_theme` -> `use crate::theme::current_theme`
  - `src/tui/widgets/file_explorer.rs`: `use crate::tui::current_theme` -> `use crate::theme::current_theme`
  - `src/recording.rs`: `use crate::branding` -> `use crate::theme` (update all `branding::` call sites to `theme::`)
- [x] Update binary crate consumers (`agr::` paths):
  - `src/main.rs`: `tui::colorize_help(...)` -> `agr::theme::colorize_help(...)` (or add `use agr::theme::colorize_help`)
  - `src/commands/config.rs`: `agr::tui::current_theme` -> `agr::theme::current_theme`, `agr::tui::theme::ansi` -> `agr::theme::ansi`
  - `src/commands/marker.rs`: `agr::tui::current_theme` -> `agr::theme::current_theme`
  - `src/commands/status.rs`: `agr::tui::current_theme` -> `agr::theme::current_theme`
  - `src/commands/shell.rs`: `agr::tui::current_theme` -> `agr::theme::current_theme`
  - `src/commands/transform.rs`: `agr::tui::current_theme` -> `agr::theme::current_theme`
  - `src/commands/agents.rs`: `agr::tui::current_theme` -> `agr::theme::current_theme`
  - `src/commands/list.rs`: `agr::tui::{current_theme, ListApp}` -> `agr::theme::current_theme` + `agr::tui::ListApp`
  - `src/commands/cleanup.rs`: `agr::tui::{current_theme, CleanupApp}` -> `agr::theme::current_theme` + `agr::tui::CleanupApp`
- [x] Update integration tests:
  - `tests/integration/snapshot_tui_test.rs`: `agr::tui::theme::current_theme` -> `agr::theme::current_theme`, `agr::tui::colorize_help` -> `agr::theme::colorize_help`
  - `tests/integration/branding_test.rs`: `agr::branding::*` -> `agr::theme::*`
  - `tests/integration/snapshot_tui_test.rs`: `agr::branding::*` -> `agr::theme::*`
- [x] Verify: `cargo check` passes

Files: ~20 files across src/ and tests/

Considerations:
- `src/commands/list.rs` and `src/commands/cleanup.rs` import both `current_theme` and a TUI app from the `tui` module. These become two separate imports.
- `src/main.rs` uses `tui::colorize_help` via the binary crate's module path -- needs careful attention.

### Stage 6: Delete old files and final verification

Goal: Remove the original files and ensure everything compiles and passes.

- [x] Delete `src/tui/theme.rs`
- [x] Delete `src/branding.rs`
- [x] Run `cargo check` -- confirm no compilation errors
- [x] Run `cargo clippy -- -D warnings` -- confirm no warnings
- [x] Run `cargo test` -- confirm all tests pass
- [x] Verify no file in `src/theme/` exceeds 400 lines

Files: `src/tui/theme.rs` (deleted), `src/branding.rs` (deleted)

Considerations:
- If `cargo check` fails after deletion, it means a consumer was missed in Stage 5 -- fix before proceeding.
- Clippy may flag unused imports in files that had dual-path imports during the migration.

## Dependencies

```
Stage 1 (mod.rs)
  |
  +-- Stage 2 (tui.rs) -- needs Theme struct from Stage 1
  |
  +-- Stage 3 (cli.rs) -- needs Theme struct from Stage 1
  |
  +-- Stage 4 (logo.rs) -- needs cli.rs items from Stage 3
  |
  +-- Stage 5 (migrate consumers) -- needs all submodules to exist
        |
        Stage 6 (delete + verify) -- needs all consumers migrated
```

Stage 2 and Stage 3 can run in parallel. Stage 4 depends on Stage 3 (uses `color_to_ansi` and `ANSI_RESET` from cli.rs). Stage 5 depends on all of 1-4. Stage 6 depends on 5.

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | done | src/theme/mod.rs created with Theme struct, presets, current_theme() |
| 2 | done | src/theme/tui.rs created with ratatui Style helpers (73 lines) |
| 3 | done | src/theme/cli.rs created with ANSI helpers, colorize_help (222 lines) |
| 4 | done | src/theme/logo.rs created with branding content (156 lines) |
| 5 | done | All ~20 consumers migrated to new import paths |
| 6 | done | Old files deleted, cargo check/clippy/test/fmt all pass |
