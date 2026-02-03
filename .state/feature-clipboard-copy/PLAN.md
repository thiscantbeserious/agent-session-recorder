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
- [x] `CopyMethod::name()` returns correct strings for all variants
- [x] `CopyResult::file_copied()` creates correct variant
- [x] `CopyResult::content_copied()` creates correct variant
- [x] `CopyResult::message()` formats file copy message correctly
- [x] `CopyResult::message()` formats content copy message correctly
- [x] `CopyResult::is_file_copy()` returns true for FileCopied
- [x] `CopyResult::is_file_copy()` returns false for ContentCopied

**error.rs tests:**
- [x] `ClipboardError::FileNotFound` displays path in message
- [x] `ClipboardError::NoToolAvailable` has helpful Linux message

**tool.rs tests:**
- [x] `CopyToolError::NotSupported` exists and is Clone
- [x] `CopyToolError::Failed` contains message string
- [x] `CopyToolError::NotFound` exists
- [x] Default `name()` implementation uses `method().name()`

#### Green Phase
- [x] Create `src/clipboard/mod.rs` with module declarations
- [x] Implement `CopyMethod` enum with all variants and `name()` method
- [x] Implement `CopyResult` enum with constructors, `message()`, `is_file_copy()`
- [x] Implement `ClipboardError` enum with thiserror derives
- [x] Implement `CopyToolError` enum
- [x] Implement `CopyTool` trait with all method signatures and default `name()`
- [x] Add `pub mod clipboard;` to `src/lib.rs`

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
- [x] MockTool compiles and implements CopyTool trait (static verification)
- [x] `Copy::with_tools()` accepts empty vec
- [x] `file()` returns `FileNotFound` for non-existent path
- [x] `file()` tries file copy first when tool supports it
- [x] `file()` returns `FileCopied` when file copy succeeds
- [x] `file()` falls back to content copy when file copy fails
- [x] `file()` returns `ContentCopied` when content copy succeeds
- [x] `file()` skips unavailable tools
- [x] `file()` skips tools that don't support file copy (for file phase)
- [x] `file()` returns `NoToolAvailable` when all tools fail
- [x] `file()` tries tools in order (first available wins)

#### Green Phase
- [x] Implement `Copy` struct with `tools` field
- [x] Implement `Copy::with_tools()` constructor
- [x] Implement `Copy::file()` with file copy phase
- [x] Implement `Copy::file()` with content copy fallback phase
- [x] Implement `Default` trait for `Copy`

**Files**: `src/clipboard/copy.rs`

**Verify**: `cargo test clipboard::copy`

---

### Stage 3: macOS Tools

**Goal**: Implement OsaScript (file copy) and Pbcopy (content copy) for macOS.

#### Red Phase

**osascript.rs tests:**
- [x] `escape_path()` handles simple path unchanged
- [x] `escape_path()` escapes double quotes
- [x] `escape_path()` escapes backslashes
- [x] `escape_path()` handles path with spaces (no escape needed)
- [x] `build_file_script()` creates correct AppleScript
- [x] `method()` returns `CopyMethod::OsaScript`
- [x] `is_available()` returns true on macOS cfg
- [x] `can_copy_files()` returns true
- [x] `try_copy_text()` returns `NotSupported`

**pbcopy.rs tests:**
- [x] `method()` returns `CopyMethod::Pbcopy`
- [x] `is_available()` returns true on macOS cfg
- [x] `can_copy_files()` returns false
- [x] `try_copy_file()` returns `NotSupported`

#### Green Phase
- [x] Create `src/clipboard/tools/mod.rs` with module declarations
- [x] Implement `OsaScript` struct with `escape_path()`, `build_file_script()`, `run_script()`
- [x] Implement `CopyTool` trait for `OsaScript`
- [x] Implement `Pbcopy` struct
- [x] Implement `CopyTool` trait for `Pbcopy` with stdin pipe to pbcopy
- [x] Implement `Default` trait for both
- [x] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/{mod,osascript,pbcopy}.rs`

**Verify**: `cargo test clipboard::tools::osascript && cargo test clipboard::tools::pbcopy`

---

### Stage 4: Linux Tools

**Goal**: Implement Xclip, Xsel, and WlCopy for Linux.

#### Red Phase

**xclip.rs tests:**
- [x] `build_file_uri()` creates correct file:// URI
- [x] `build_file_uri()` handles paths with spaces (URI encoding)
- [x] `method()` returns `CopyMethod::Xclip`
- [x] `is_available()` checks for xclip binary
- [x] `can_copy_files()` returns true

**xsel.rs tests:**
- [x] `method()` returns `CopyMethod::Xsel`
- [x] `is_available()` checks for xsel binary
- [x] `can_copy_files()` returns false (xsel is text-only)
- [x] `try_copy_file()` returns `NotSupported`

**wl_copy.rs tests:**
- [x] `method()` returns `CopyMethod::WlCopy`
- [x] `is_available()` checks for wl-copy binary
- [x] `can_copy_files()` returns false (wl-copy is text-only for our use)
- [x] `try_copy_file()` returns `NotSupported`

#### Green Phase
- [x] Implement `Xclip` struct with `build_file_uri()` helper
- [x] Implement `CopyTool` trait for `Xclip` with `-t text/uri-list` for files
- [x] Implement `Xsel` struct
- [x] Implement `CopyTool` trait for `Xsel` with `--clipboard --input`
- [x] Implement `WlCopy` struct
- [x] Implement `CopyTool` trait for `WlCopy` with stdin pipe
- [x] Implement `Default` trait for all
- [x] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/{xclip,xsel,wl_copy}.rs`

**Verify**: `cargo test clipboard::tools::xclip && cargo test clipboard::tools::xsel && cargo test clipboard::tools::wl_copy`

---

### Stage 5: Platform Selection & Public API

**Goal**: Implement `platform_tools()`, `tool_exists()`, wire up `Copy::new()`, and expose public API.

#### Red Phase

**tools/mod.rs tests:**
- [x] `tool_exists()` returns false for nonexistent tool (implemented per-tool)
- [x] `platform_tools()` returns OsaScript, Pbcopy on macOS
- [x] `platform_tools()` returns Xclip, Xsel, WlCopy on Linux
- [x] `platform_tools()` returns empty vec on other platforms

**mod.rs tests:**
- [x] `copy_file_to_clipboard()` returns error for non-existent file
- [x] `copy_file_to_clipboard()` delegates to `Copy::new().file()`

#### Green Phase
- [x] Implement `tool_exists()` using `which` command (per-tool implementation)
- [x] Implement `platform_tools()` with cfg attributes
- [x] Wire up `Copy::new()` to use `platform_tools()`
- [x] Implement `copy_file_to_clipboard()` convenience function
- [x] Ensure all public types are re-exported in `src/clipboard/mod.rs`
- [x] Add module documentation

**Files**: `src/clipboard/{tools/mod,mod}.rs`

**Verify**: `cargo test clipboard && cargo doc --no-deps`

---

### Stage 6: CLI Integration

**Goal**: Add `agr copy` command with handler and dispatch.

**Important**: The argument MUST be named `file` (not `recording`) to enable automatic shell completion detection.

#### Red Phase

**cli.rs tests:**
- [x] `agr copy --help` parses successfully
- [x] `agr copy session.cast` parses with correct file argument
- [x] `agr copy` without args shows error

**commands/copy.rs tests:**
- [x] Handler returns error for non-existent file with helpful message
- [x] Handler accepts filename with and without .cast extension
- [x] Handler resolves short format paths (agent/file.cast)

**shell/completions.rs tests:**
- [x] `extract_commands()` includes `copy` command
- [x] `copy` command has `accepts_file == true`

#### Green Phase
- [x] Add `Copy { file: String }` variant to `Commands` enum in `src/cli.rs`
  - **Critical**: Name the argument `file`, not `recording`
  - **Note**: `Copy` as variant name is valid but potentially confusing (Rust `Copy` trait exists). Acceptable.
- [x] Add help text and long_about with examples
- [x] Create `src/commands/copy.rs` with `handle(file: &str) -> Result<()>`
  - Load config
  - Use `resolve_file_path()` for path resolution
  - Validate file exists with helpful error
  - Call `clipboard::copy_file_to_clipboard()`
  - **Strip `.cast` extension** from filename before calling `result.message()` (it appends `.cast` internally)
  - Print themed result message
- [x] Export module in `src/commands/mod.rs`
- [x] Add match arm for `Commands::Copy { file }` in `main.rs`

**Files**: `src/{cli,main}.rs`, `src/commands/{mod,copy}.rs`

**Verify**: `cargo test cli && cargo test commands::copy && cargo test shell::completions && cargo run -- copy --help`

---

### Stage 7: TUI Integration

**Goal**: Add Copy to context menu with keybinding and help text.

#### Red Phase

**Context menu tests:**
- [x] `ContextMenuItem::Copy` exists
- [x] `ContextMenuItem::Copy.label()` returns "Copy to clipboard"
- [x] `ContextMenuItem::Copy.shortcut()` returns "c"
- [x] `ContextMenuItem::ALL` has 6 items
- [x] `ContextMenuItem::ALL[1]` is Copy (after Play)

**Action handler tests:**
- [x] `c` key in Normal mode with selection triggers copy
- [x] `c` key in ContextMenu mode selects Copy and executes
- [x] Copy action sets status message on success
- [x] Copy action sets error status message on failure

**Help text tests:**
- [x] Snapshot test: help modal includes "c" and "Copy" text

#### Green Phase
- [x] Add `Copy` variant to `ContextMenuItem` enum
- [x] Update `ContextMenuItem::ALL` array (6 items, Copy at index 1)
- [x] Implement `label()` match arm: "Copy to clipboard"
- [x] Implement `shortcut()` match arm: "c"
- [x] Add `copy_to_clipboard(&mut self) -> Result<()>` method to `ListApp`
- [x] Add `KeyCode::Char('c')` handling in `handle_normal_key()`
- [x] Add `KeyCode::Char('c')` handling in `handle_context_menu_key()`
- [x] Wire up `ContextMenuItem::Copy` in `execute_context_menu_action()`
- [x] Add copy keybinding line to `render_help_modal()` in Actions section
- [x] Update `modal_height` calculation if needed

**Files**: `src/tui/list_app.rs`

**Verify**: `cargo test tui::list_app && cargo test snapshot`

---

### Stage 8: Documentation

**Goal**: Document copy feature for users.

#### README.md
- [x] Add `agr copy` example to Quick Start section
- [x] Add copy command documentation section with examples
- [x] Document `c` keybinding in TUI controls section
- [x] Add note about platform behavior (macOS file copy vs Linux content fallback)

#### Generated Docs
- [x] Run `cargo xtask gen-docs`
- [x] Verify copy command appears in `docs/COMMANDS.md`
- [x] Verify wiki pages updated if applicable
- [x] Review diff for accuracy

**Files**: `README.md`, `docs/COMMANDS.md`, `docs/wiki/*`

**Verify**: `cargo xtask gen-docs && git diff docs/`

---

### Stage 9: Integration Tests

**Goal**: End-to-end verification including shell completions.

#### Tests
- [x] Create `tests/integration/copy_test.rs`
- [x] Test: `agr copy --help` exits 0 and shows usage
- [x] Test: `agr copy nonexistent.cast` exits non-zero with helpful error
- [x] Test: `agr copy` without arguments shows error
- [x] Test: path resolution works with short format
- [x] Test: (cfg macos) copy succeeds with temp file
- [x] Test: (cfg linux) copy succeeds or fails gracefully based on tools
- [x] Test: `agr completions --files` includes test recordings
- [x] Test: generated zsh init contains "copy" in `_agr_file_cmds`
- [x] Test: generated bash init contains "copy" in `_agr_file_cmds`

**Files**: `tests/integration/copy_test.rs`

**Verify**: `cargo test --test copy_test`

---

### Stage 10: Manual Platform Testing

**Goal**: Verify real-world behavior on all supported platforms.

#### macOS
- [x] Run `agr copy <file>`, paste into Slack, verify file attachment works
- [x] Run `agr ls`, press `c`, paste into Slack
- [x] Test `agr copy <TAB>` shows recording completions
- [ ] Test fallback: temporarily rename osascript, verify pbcopy content copy works (skipped - not critical)

#### Linux X11
- [ ] Test with xclip installed (requires Linux environment)
- [ ] Test with only xsel installed (content fallback)
- [ ] Test `agr copy <TAB>` shows recording completions

#### Linux Wayland
- [ ] Test with wl-copy installed (requires Wayland environment)

#### Linux (no tools)
- [ ] Verify helpful error message when no clipboard tools installed

**Verify**: Manual testing on each platform - macOS complete, Linux deferred to CI/community

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
| 1 | complete | Core types: result, error, tool trait |
| 2 | complete | Copy orchestrator with MockTool |
| 3 | complete | macOS: OsaScript + Pbcopy |
| 4 | complete | Linux: Xclip + Xsel + WlCopy |
| 5 | complete | Platform selection + public API |
| 6 | complete | CLI (arg must be named `file` for completions) |
| 7 | complete | TUI: menu + action + help |
| 8 | complete | Documentation: README + gen-docs |
| 9 | complete | Integration tests: CLI + completions |
| 10 | complete | macOS tested: copy+paste to Slack works, shell completions work |

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
