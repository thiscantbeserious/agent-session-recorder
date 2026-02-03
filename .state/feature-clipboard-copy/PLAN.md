# Plan: Clipboard Copy Feature

References: ADR.md, REQUIREMENTS.md

## Open Questions

Implementation challenges for the implementer to resolve:

1. **xclip file URI format**: Test whether xclip requires `file:///path` (3 slashes) or `file://path` (2 slashes) on actual Linux system.
2. **osascript quoting edge cases**: Paths with newlines or null bytes - decide whether to reject or escape.
3. **Large file content warning**: Consider warning for files >1MB when falling back to content copy.

---

## TDD Methodology

All stages follow TDD:
- **Red**: Write failing tests first
- **Green**: Implement minimal code to pass tests
- **Refactor**: Clean up while keeping tests green

---

## Stages

### Stage 1: Core Types

**Goal**: Define `CopyResult`, `CopyMethod`, `ClipboardError`, `CopyTool` trait, and `CopyToolError`.

#### Red Phase

**result.rs tests:**
- [ ] `CopyMethod::name()` returns correct strings for all variants
- [ ] `CopyResult::file_copied()` creates correct variant
- [ ] `CopyResult::content_copied()` creates correct variant
- [ ] `CopyResult::message()` formats file copy message correctly
- [ ] `CopyResult::message()` formats content copy message correctly
- [ ] `CopyResult::is_file_copy()` returns true for FileCopied
- [ ] `CopyResult::is_file_copy()` returns false for ContentCopied

**error.rs tests:**
- [ ] `ClipboardError::FileNotFound` displays path in message
- [ ] `ClipboardError::NoToolAvailable` has helpful Linux message

**tool.rs tests:**
- [ ] `CopyToolError::NotSupported` exists and is Clone
- [ ] `CopyToolError::Failed` contains message string
- [ ] `CopyToolError::NotFound` exists
- [ ] Default `name()` implementation uses `method().name()`

#### Green Phase
- [ ] Create `src/clipboard/mod.rs` with module declarations
- [ ] Implement `CopyMethod` enum with all variants and `name()` method
- [ ] Implement `CopyResult` enum with constructors, `message()`, `is_file_copy()`
- [ ] Implement `ClipboardError` enum with thiserror derives
- [ ] Implement `CopyToolError` enum
- [ ] Implement `CopyTool` trait with all method signatures and default `name()`
- [ ] Add `pub mod clipboard;` to `src/lib.rs`

**Files**: `src/clipboard/{mod,result,error,tool}.rs`, `src/lib.rs`

**Verify**: `cargo test clipboard::result && cargo test clipboard::error && cargo test clipboard::tool`

---

### Stage 2: Copy Orchestrator

**Goal**: Implement the `Copy` struct that tries tools in order.

#### Red Phase

Create MockTool for testing:
```rust
struct MockTool {
    method: CopyMethod,
    available: bool,
    can_files: bool,
    file_result: Result<(), CopyToolError>,
    text_result: Result<(), CopyToolError>,
}
```

**Tests:**
- [ ] MockTool compiles and implements CopyTool trait (static verification)
- [ ] `Copy::with_tools()` accepts empty vec
- [ ] `file()` returns `FileNotFound` for non-existent path
- [ ] `file()` tries file copy first when tool supports it
- [ ] `file()` returns `FileCopied` when file copy succeeds
- [ ] `file()` falls back to content copy when file copy fails
- [ ] `file()` returns `ContentCopied` when content copy succeeds
- [ ] `file()` skips unavailable tools
- [ ] `file()` skips tools that don't support file copy (for file phase)
- [ ] `file()` returns `NoToolAvailable` when all tools fail
- [ ] `file()` tries tools in order (first available wins)

#### Green Phase
- [ ] Implement `Copy` struct with `tools` field
- [ ] Implement `Copy::with_tools()` constructor
- [ ] Implement `Copy::file()` with file copy phase
- [ ] Implement `Copy::file()` with content copy fallback phase
- [ ] Implement `Default` trait for `Copy`

**Files**: `src/clipboard/copy.rs`

**Verify**: `cargo test clipboard::copy`

---

### Stage 3: macOS Tools

**Goal**: Implement OsaScript (file copy) and Pbcopy (content copy) for macOS.

#### Red Phase

**osascript.rs tests:**
- [ ] `escape_path()` handles simple path unchanged
- [ ] `escape_path()` escapes double quotes
- [ ] `escape_path()` escapes backslashes
- [ ] `escape_path()` handles path with spaces (no escape needed)
- [ ] `build_file_script()` creates correct AppleScript
- [ ] `method()` returns `CopyMethod::OsaScript`
- [ ] `is_available()` returns true on macOS cfg
- [ ] `can_copy_files()` returns true
- [ ] `try_copy_text()` returns `NotSupported`

**pbcopy.rs tests:**
- [ ] `method()` returns `CopyMethod::Pbcopy`
- [ ] `is_available()` returns true on macOS cfg
- [ ] `can_copy_files()` returns false
- [ ] `try_copy_file()` returns `NotSupported`

#### Green Phase
- [ ] Create `src/clipboard/tools/mod.rs` with module declarations
- [ ] Implement `OsaScript` struct with `escape_path()`, `build_file_script()`, `run_script()`
- [ ] Implement `CopyTool` trait for `OsaScript`
- [ ] Implement `Pbcopy` struct
- [ ] Implement `CopyTool` trait for `Pbcopy` with stdin pipe to pbcopy
- [ ] Implement `Default` trait for both
- [ ] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/{mod,osascript,pbcopy}.rs`

**Verify**: `cargo test clipboard::tools::osascript && cargo test clipboard::tools::pbcopy`

---

### Stage 4: Linux Tools

**Goal**: Implement Xclip, Xsel, and WlCopy for Linux.

#### Red Phase

**xclip.rs tests:**
- [ ] `build_file_uri()` creates correct file:// URI
- [ ] `build_file_uri()` handles paths with spaces (URI encoding)
- [ ] `method()` returns `CopyMethod::Xclip`
- [ ] `is_available()` checks for xclip binary
- [ ] `can_copy_files()` returns true

**xsel.rs tests:**
- [ ] `method()` returns `CopyMethod::Xsel`
- [ ] `is_available()` checks for xsel binary
- [ ] `can_copy_files()` returns false (xsel is text-only)
- [ ] `try_copy_file()` returns `NotSupported`

**wl_copy.rs tests:**
- [ ] `method()` returns `CopyMethod::WlCopy`
- [ ] `is_available()` checks for wl-copy binary
- [ ] `can_copy_files()` returns false (wl-copy is text-only for our use)
- [ ] `try_copy_file()` returns `NotSupported`

#### Green Phase
- [ ] Implement `Xclip` struct with `build_file_uri()` helper
- [ ] Implement `CopyTool` trait for `Xclip` with `-t text/uri-list` for files
- [ ] Implement `Xsel` struct
- [ ] Implement `CopyTool` trait for `Xsel` with `--clipboard --input`
- [ ] Implement `WlCopy` struct
- [ ] Implement `CopyTool` trait for `WlCopy` with stdin pipe
- [ ] Implement `Default` trait for all
- [ ] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/{xclip,xsel,wl_copy}.rs`

**Verify**: `cargo test clipboard::tools::xclip && cargo test clipboard::tools::xsel && cargo test clipboard::tools::wl_copy`

---

### Stage 5: Platform Selection & Public API

**Goal**: Implement `platform_tools()`, `tool_exists()`, wire up `Copy::new()`, and expose public API.

#### Red Phase

**tools/mod.rs tests:**
- [ ] `tool_exists()` returns false for nonexistent tool
- [ ] `platform_tools()` returns OsaScript, Pbcopy on macOS
- [ ] `platform_tools()` returns Xclip, Xsel, WlCopy on Linux
- [ ] `platform_tools()` returns empty vec on other platforms

**mod.rs tests:**
- [ ] `copy_file_to_clipboard()` returns error for non-existent file
- [ ] `copy_file_to_clipboard()` delegates to `Copy::new().file()`

#### Green Phase
- [ ] Implement `tool_exists()` using `which` command
- [ ] Implement `platform_tools()` with cfg attributes
- [ ] Wire up `Copy::new()` to use `platform_tools()`
- [ ] Implement `copy_file_to_clipboard()` convenience function
- [ ] Ensure all public types are re-exported in `src/clipboard/mod.rs`
- [ ] Add module documentation

**Files**: `src/clipboard/{tools/mod,mod}.rs`

**Verify**: `cargo test clipboard && cargo doc --no-deps`

---

### Stage 6: CLI Integration

**Goal**: Add `agr copy` command with handler and dispatch.

**Important**: The argument MUST be named `file` (not `recording`) to enable automatic shell completion detection.

#### Red Phase

**cli.rs tests:**
- [ ] `agr copy --help` parses successfully
- [ ] `agr copy session.cast` parses with correct file argument
- [ ] `agr copy` without args shows error

**commands/copy.rs tests:**
- [ ] Handler returns error for non-existent file with helpful message
- [ ] Handler accepts filename with and without .cast extension
- [ ] Handler resolves short format paths (agent/file.cast)

**shell/completions.rs tests:**
- [ ] `extract_commands()` includes `copy` command
- [ ] `copy` command has `accepts_file == true`

#### Green Phase
- [ ] Add `Copy { file: String }` variant to `Commands` enum in `src/cli.rs`
  - **Critical**: Name the argument `file`, not `recording`
  - **Note**: `Copy` as variant name is valid but potentially confusing (Rust `Copy` trait exists). Acceptable.
- [ ] Add help text and long_about with examples
- [ ] Create `src/commands/copy.rs` with `handle(file: &str) -> Result<()>`
  - Load config
  - Use `resolve_file_path()` for path resolution
  - Validate file exists with helpful error
  - Call `clipboard::copy_file_to_clipboard()`
  - **Strip `.cast` extension** from filename before calling `result.message()` (it appends `.cast` internally)
  - Print themed result message
- [ ] Export module in `src/commands/mod.rs`
- [ ] Add match arm for `Commands::Copy { file }` in `main.rs`

**Files**: `src/{cli,main}.rs`, `src/commands/{mod,copy}.rs`

**Verify**: `cargo test cli && cargo test commands::copy && cargo test shell::completions && cargo run -- copy --help`

---

### Stage 7: TUI Integration

**Goal**: Add Copy to context menu with keybinding and help text.

#### Red Phase

**Context menu tests:**
- [ ] `ContextMenuItem::Copy` exists
- [ ] `ContextMenuItem::Copy.label()` returns "Copy to clipboard"
- [ ] `ContextMenuItem::Copy.shortcut()` returns "c"
- [ ] `ContextMenuItem::ALL` has 6 items
- [ ] `ContextMenuItem::ALL[1]` is Copy (after Play)

**Action handler tests:**
- [ ] `c` key in Normal mode with selection triggers copy
- [ ] `c` key in ContextMenu mode selects Copy and executes
- [ ] Copy action sets status message on success
- [ ] Copy action sets error status message on failure

**Help text tests:**
- [ ] Snapshot test: help modal includes "c" and "Copy" text

#### Green Phase
- [ ] Add `Copy` variant to `ContextMenuItem` enum
- [ ] Update `ContextMenuItem::ALL` array (6 items, Copy at index 1)
- [ ] Implement `label()` match arm: "Copy to clipboard"
- [ ] Implement `shortcut()` match arm: "c"
- [ ] Add `copy_to_clipboard(&mut self) -> Result<()>` method to `ListApp`
- [ ] Add `KeyCode::Char('c')` handling in `handle_normal_key()`
- [ ] Add `KeyCode::Char('c')` handling in `handle_context_menu_key()`
- [ ] Wire up `ContextMenuItem::Copy` in `execute_context_menu_action()`
- [ ] Add copy keybinding line to `render_help_modal()` in Actions section
- [ ] Update `modal_height` calculation if needed

**Files**: `src/tui/list_app.rs`

**Verify**: `cargo test tui::list_app && cargo test snapshot`

---

### Stage 8: Documentation

**Goal**: Document copy feature for users.

#### README.md
- [ ] Add `agr copy` example to Quick Start section
- [ ] Add copy command documentation section with examples
- [ ] Document `c` keybinding in TUI controls section
- [ ] Add note about platform behavior (macOS file copy vs Linux content fallback)

#### Generated Docs
- [ ] Run `cargo xtask gen-docs`
- [ ] Verify copy command appears in `docs/COMMANDS.md`
- [ ] Verify wiki pages updated if applicable
- [ ] Review diff for accuracy

**Files**: `README.md`, `docs/COMMANDS.md`, `docs/wiki/*`

**Verify**: `cargo xtask gen-docs && git diff docs/`

---

### Stage 9: Integration Tests

**Goal**: End-to-end verification including shell completions.

#### Tests
- [ ] Create `tests/integration/copy_test.rs`
- [ ] Test: `agr copy --help` exits 0 and shows usage
- [ ] Test: `agr copy nonexistent.cast` exits non-zero with helpful error
- [ ] Test: `agr copy` without arguments shows error
- [ ] Test: path resolution works with short format
- [ ] Test: (cfg macos) copy succeeds with temp file
- [ ] Test: (cfg linux) copy succeeds or fails gracefully based on tools
- [ ] Test: `agr completions --files` includes test recordings
- [ ] Test: generated zsh init contains "copy" in `_agr_file_cmds`
- [ ] Test: generated bash init contains "copy" in `_agr_file_cmds`

**Files**: `tests/integration/copy_test.rs`

**Verify**: `cargo test --test copy_test`

---

### Stage 10: Manual Platform Testing

**Goal**: Verify real-world behavior on all supported platforms.

#### macOS
- [ ] Run `agr copy <file>`, paste into Slack, verify file attachment works
- [ ] Run `agr ls`, press `c`, paste into Slack
- [ ] Test `agr copy <TAB>` shows recording completions
- [ ] Test fallback: temporarily rename osascript, verify pbcopy content copy works

#### Linux X11
- [ ] Test with xclip installed
- [ ] Test with only xsel installed (content fallback)
- [ ] Test `agr copy <TAB>` shows recording completions

#### Linux Wayland
- [ ] Test with wl-copy installed

#### Linux (no tools)
- [ ] Verify helpful error message when no clipboard tools installed

**Verify**: Manual testing on each platform

---

## Dependencies

```
Stage 1 (Types) ──> Stage 2 (Orchestrator) ──┬──> Stage 3 (macOS Tools)
                                              └──> Stage 4 (Linux Tools)
                                                        │
                                              ┌─────────┴─────────┐
                                              v                   v
                                       Stage 5 (API) ─────────────┤
                                              │                   │
                              ┌───────────────┴───────────────┐   │
                              v                               v   │
                       Stage 6 (CLI)                   Stage 7 (TUI)
                              │                               │
                              └───────────┬───────────────────┘
                                          v
                                   Stage 8 (Documentation)
                                          │
                                          v
                                   Stage 9 (Integration Tests)
                                          │
                                          v
                                   Stage 10 (Manual Testing)
```

**Parallelization opportunities:**
- Stages 3+4 (macOS + Linux tools) can run in parallel
- Stages 6+7 (CLI + TUI) can run in parallel after Stage 5

---

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | pending | Core types: result, error, tool trait |
| 2 | pending | Copy orchestrator with MockTool |
| 3 | pending | macOS: OsaScript + Pbcopy |
| 4 | pending | Linux: Xclip + Xsel + WlCopy |
| 5 | pending | Platform selection + public API |
| 6 | pending | CLI (arg must be named `file` for completions) |
| 7 | pending | TUI: menu + action + help |
| 8 | pending | Documentation |
| 9 | pending | Integration tests |
| 10 | pending | Manual platform testing |

---

## Test Commands

```bash
# Run all clipboard tests
cargo test clipboard

# Run specific module tests
cargo test clipboard::result
cargo test clipboard::error
cargo test clipboard::tool
cargo test clipboard::copy
cargo test clipboard::tools

# Run CLI/TUI tests
cargo test cli
cargo test commands::copy
cargo test tui::list_app
cargo test shell::completions

# Run integration tests
cargo test --test copy_test

# Regenerate docs
cargo xtask gen-docs

# Manual verification
cargo run -- copy --help
cargo run -- copy test.cast
cargo run -- ls   # Then press 'c' on a selection

# Shell completion verification
cargo run -- completions --files ""           # Should list all recordings
cargo run -- completions --files "claude/"    # Should filter by agent
agr copy <TAB>                                # Should show completions (after shell install)
```
