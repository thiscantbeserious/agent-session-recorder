# ADR: Consolidate theme and branding into a top-level theme module

## Status

Accepted

## Context

Visual presentation code is split across two unrelated locations:

- `src/tui/theme.rs` (291 lines) -- Theme struct, color palette presets, ratatui Style helpers, ANSI color conversion, CLI help colorization, and `current_theme()`.
- `src/branding.rs` (161 lines) -- Logo assets (`include_str!`), banner printing, box-drawing helpers, and Unicode string truncation.

**Why this is a problem:**

1. `theme.rs` lives under `tui/` but is consumed by the entire codebase: CLI commands, recording, branding, and TUI widgets. It is not TUI-specific.
2. `branding.rs` sits at the crate root despite being a pure presentation concern.
3. The dependency direction (`branding` -> `tui::theme`) crosses module boundaries in a way that obscures the relationship.

Both modules deal with "how things look" and belong together under a single, top-level `theme` module.

### Constraints

- Pure refactoring -- zero behavior changes.
- All existing tests must pass without modification to assertions (only import paths change).
- `include_str!` paths must resolve correctly after the move (relative to new file location).
- No file exceeds ~400 lines.

## Options Considered

### Option A: Split `impl Theme` across submodule files (Selected)

Each submodule file contains its own `impl Theme` block with only the methods relevant to that concern.

```
src/theme/
  mod.rs    -- Theme struct, palette fields, presets (claude_code/classic/ocean),
               current_theme(), Default impl, re-exports (~70 lines)
  tui.rs    -- impl Theme { ratatui Style helpers } (~50 lines)
  cli.rs    -- impl Theme { ANSI text helpers }, color_to_ansi(), ANSI_RESET,
               ansi module, colorize_help(), find_description_start() (~130 lines)
  logo.rs   -- Logo consts, banner print functions, box-drawing helpers,
               truncate_str() (~100 lines)
```

- **Pros:** Each file is self-contained for its concern. Rust natively supports split `impl` blocks. Easy to find "all TUI styles" or "all CLI formatting" by opening one file. No awkward delegation or wrapper functions.
- **Cons:** `impl Theme` split across files is less common in Rust. A developer looking for a method must check multiple files (mitigated by IDE go-to-definition and re-exports).

### Option B: Keep all `impl Theme` in mod.rs, put free functions in submodules

```
src/theme/
  mod.rs    -- Theme struct + ALL impl methods (~150 lines)
  cli.rs    -- color_to_ansi(), ANSI_RESET, ansi module, colorize_help() (~90 lines)
  logo.rs   -- Logo consts, banner functions, box-drawing, truncate_str() (~100 lines)
```

No `tui.rs` submodule -- the ratatui Style helpers stay in mod.rs alongside the struct.

- **Pros:** All Theme methods in one place. Simpler module structure (3 files not 4). Familiar single-impl pattern.
- **Cons:** mod.rs grows toward ~150 lines with mixed TUI/core concerns. Harder to separate "what needs ratatui" from "what is pure data."

### Option C: Flat re-export everything from mod.rs

Same file split as Option A, but mod.rs re-exports every public item so consumers always import from `theme::*` or `crate::theme::SomeItem`.

- **Pros:** Single import namespace, backward-compatible feel.
- **Cons:** Long re-export list that must be maintained. Hides module structure from consumers.

## Decision

**Option A: Split `impl Theme` across submodule files**, with **selective re-exports** in mod.rs.

### Re-export strategy

mod.rs re-exports the most commonly used items for convenience:

```rust
// From cli.rs -- used by main.rs, commands/*, branding consumers
pub use cli::{colorize_help, color_to_ansi, ANSI_RESET};
pub use cli::ansi;

// From logo.rs -- used by recording.rs, integration tests
pub use logo::{
    print_start_banner, print_done_banner, print_full_logo,
    print_box_line, print_box_bottom, print_box_prompt, print_box_line_end,
    truncate_str, BOX_BOTTOM, BOX_WIDTH, LOGO_DONE, LOGO_FULL, LOGO_START,
};
```

TUI style helpers are **not** re-exported from mod.rs. TUI widgets already import `current_theme()` and call methods on the returned `Theme` directly, so no separate re-export is needed for the Style methods.

### What goes where (detailed)

**mod.rs** (~70 lines):
- `pub struct Theme { ... }` with all 6 palette fields
- `impl Default for Theme` (delegates to `claude_code()`)
- `impl Theme { fn claude_code(), fn classic(), fn ocean() }` -- presets
- `pub fn current_theme() -> Theme`
- Module declarations and re-exports

**tui.rs** (~50 lines):
- `use ratatui::style::{Color, Modifier, Style};`
- `impl Theme { text_style, text_secondary_style, accent_style, accent_bold_style, error_style, success_style, highlight_style }`
- These are the only methods that depend on `ratatui`

**cli.rs** (~130 lines):
- `pub const ANSI_RESET`
- `pub mod ansi { GRAY, GREEN, RED, DARK_GRAY, RESET }`
- `pub fn color_to_ansi(color: Color) -> &'static str`
- `impl Theme { accent_text, primary_text, secondary_text, error_text, success_text }`
- `pub fn colorize_help(text: &str) -> String`
- `fn find_description_start(line: &str) -> Option<usize>`

**logo.rs** (~100 lines):
- `pub const LOGO_FULL/LOGO_START/LOGO_DONE` with updated `include_str!("../../assets/...")` paths
- `pub const BOX_WIDTH/BOX_BOTTOM`
- All `print_*` functions (start_banner, done_banner, full_logo, box_line, box_bottom, box_prompt, box_line_end)
- `fn colorize_recording_banner(...)` (private helper)
- `pub fn truncate_str(...)`

### Import migration summary

| Current import | New import |
|---|---|
| `crate::tui::theme::Theme` | `crate::theme::Theme` |
| `crate::tui::theme::current_theme` | `crate::theme::current_theme` |
| `crate::tui::theme::{color_to_ansi, ANSI_RESET}` | `crate::theme::{color_to_ansi, ANSI_RESET}` |
| `crate::tui::theme::ansi` | `crate::theme::ansi` |
| `agr::tui::current_theme` | `agr::theme::current_theme` |
| `agr::tui::colorize_help` | `agr::theme::colorize_help` |
| `agr::tui::theme::current_theme` | `agr::theme::current_theme` |
| `agr::tui::theme::ansi` | `agr::theme::ansi` |
| `crate::branding` | `crate::theme` (logo items re-exported) |
| `agr::branding::*` | `agr::theme::*` (logo items re-exported) |

### tui/mod.rs changes

- Remove `pub mod theme;`
- Replace re-exports to point at the new location:
  ```rust
  // Backward-compatible re-exports (deprecated path)
  // Or simply remove and fix all consumers
  ```
- Decision: **Remove re-exports from tui/mod.rs entirely** and update all consumers. This is a clean break -- re-exporting from the old location would perpetuate the wrong mental model.

## Consequences

### What becomes easier

- Finding all presentation code: one `src/theme/` directory.
- Understanding dependencies: `tui` module is purely about ratatui widgets, `theme` is about colors/styles/logos.
- Adding new visual concerns (e.g., progress bar styles): clear home in `src/theme/`.

### What becomes harder

- Nothing significant. One-time import path churn across ~20 files.

### Risks

- `include_str!` path change (`../assets/` -> `../../assets/`) could break if paths are wrong. Mitigated by compiler error at build time.

## Decision History

1. User specified the target structure in the task description.
2. Architect chose Option A (split impl blocks) over Option B (monolithic impl) because it keeps each file focused on a single concern and ratatui dependency isolated to `tui.rs`.
3. Re-exports chosen over full-path imports to minimize consumer churn for commonly-used items.
4. tui/mod.rs re-exports removed entirely (clean break, not deprecated path).
