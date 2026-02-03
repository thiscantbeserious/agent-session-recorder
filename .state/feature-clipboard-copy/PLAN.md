# Plan: Clipboard Copy Feature

References: ADR.md, REQUIREMENTS.md

## Open Questions

Implementation challenges for the implementer to resolve:

1. **xclip file URI format**: Test whether xclip requires `file:///path` (3 slashes) or `file://path` (2 slashes) on actual Linux system.
2. **osascript quoting edge cases**: Paths with newlines or null bytes - decide whether to reject or escape.
3. **Large file content warning**: Consider warning for files >1MB when falling back to content copy.

---

## Stages

All stages follow TDD methodology:
- **Red**: Write failing tests first
- **Green**: Implement minimal code to pass tests
- **Refactor**: Clean up while keeping tests green

---

### Stage 1: Core Types (result.rs, error.rs)

**Goal**: Define `CopyResult`, `CopyMethod`, and `ClipboardError` types.

#### Red Phase
- [ ] Create `src/clipboard/mod.rs` with module declarations (empty)
- [ ] Create `src/clipboard/result.rs` with type stubs
- [ ] Write test: `CopyMethod::name()` returns correct strings for all variants
- [ ] Write test: `CopyResult::file_copied()` creates correct variant
- [ ] Write test: `CopyResult::content_copied()` creates correct variant
- [ ] Write test: `CopyResult::message()` formats file copy message correctly
- [ ] Write test: `CopyResult::message()` formats content copy message correctly
- [ ] Write test: `CopyResult::is_file_copy()` returns true for FileCopied
- [ ] Write test: `CopyResult::is_file_copy()` returns false for ContentCopied
- [ ] Create `src/clipboard/error.rs` with error stubs
- [ ] Write test: `ClipboardError::FileNotFound` displays path in message
- [ ] Write test: `ClipboardError::NoToolAvailable` has helpful Linux message

#### Green Phase
- [ ] Implement `CopyMethod` enum with all variants
- [ ] Implement `CopyMethod::name()` method
- [ ] Implement `CopyResult` enum with constructors
- [ ] Implement `CopyResult::message()` method
- [ ] Implement `CopyResult::is_file_copy()` method
- [ ] Implement `ClipboardError` enum with thiserror derives
- [ ] Export types in `src/clipboard/mod.rs`
- [ ] Add `pub mod clipboard;` to `src/lib.rs`

**Files**: `src/clipboard/mod.rs`, `src/clipboard/result.rs`, `src/clipboard/error.rs`, `src/lib.rs`

**Verify**: `cargo test clipboard::result && cargo test clipboard::error`

---

### Stage 2: CopyTool Trait (tool.rs)

**Goal**: Define the `CopyTool` trait and `CopyToolError` type.

#### Red Phase
- [ ] Create `src/clipboard/tool.rs` with trait stub
- [ ] Write test: `CopyToolError::NotSupported` exists and is Clone
- [ ] Write test: `CopyToolError::Failed` contains message string
- [ ] Write test: `CopyToolError::NotFound` exists
- [ ] Write test: default `name()` implementation uses `method().name()`

#### Green Phase
- [ ] Implement `CopyToolError` enum
- [ ] Implement `CopyTool` trait with all method signatures
- [ ] Implement default `name()` method on trait
- [ ] Export in `src/clipboard/mod.rs`

**Files**: `src/clipboard/tool.rs`, `src/clipboard/mod.rs`

**Verify**: `cargo test clipboard::tool`

---

### Stage 3: Copy Orchestrator (copy.rs)

**Goal**: Implement the `Copy` struct that tries tools in order.

#### Red Phase
- [ ] Create `src/clipboard/copy.rs` with struct stub
- [ ] Create mock tool for testing:
  ```rust
  struct MockTool {
      method: CopyMethod,
      available: bool,
      can_files: bool,
      file_result: Result<(), CopyToolError>,
      text_result: Result<(), CopyToolError>,
  }
  ```
- [ ] Write test: `Copy::with_tools()` accepts empty vec
- [ ] Write test: `file()` returns `FileNotFound` for non-existent path
- [ ] Write test: `file()` tries file copy first when tool supports it
- [ ] Write test: `file()` returns `FileCopied` when file copy succeeds
- [ ] Write test: `file()` falls back to content copy when file copy fails
- [ ] Write test: `file()` returns `ContentCopied` when content copy succeeds
- [ ] Write test: `file()` skips unavailable tools
- [ ] Write test: `file()` skips tools that don't support file copy (for file phase)
- [ ] Write test: `file()` returns `NoToolAvailable` when all tools fail
- [ ] Write test: `file()` tries tools in order (first available wins)

#### Green Phase
- [ ] Implement `Copy` struct with `tools` field
- [ ] Implement `Copy::with_tools()` constructor
- [ ] Implement `Copy::file()` with file copy phase
- [ ] Implement `Copy::file()` with content copy fallback phase
- [ ] Implement `Default` trait for `Copy`
- [ ] Export in `src/clipboard/mod.rs`

**Files**: `src/clipboard/copy.rs`, `src/clipboard/mod.rs`

**Verify**: `cargo test clipboard::copy`

---

### Stage 4: OsaScript Tool (tools/osascript.rs)

**Goal**: Implement macOS file copy via osascript.

#### Red Phase
- [ ] Create `src/clipboard/tools/mod.rs` with module declarations
- [ ] Create `src/clipboard/tools/osascript.rs` with struct stub
- [ ] Write test: `escape_path()` handles simple path unchanged
- [ ] Write test: `escape_path()` escapes double quotes
- [ ] Write test: `escape_path()` escapes backslashes
- [ ] Write test: `escape_path()` handles path with spaces (no escape needed)
- [ ] Write test: `build_file_script()` creates correct AppleScript
- [ ] Write test: `method()` returns `CopyMethod::OsaScript`
- [ ] Write test: `is_available()` returns true on macOS cfg
- [ ] Write test: `can_copy_files()` returns true
- [ ] Write test: `try_copy_text()` returns `NotSupported`

#### Green Phase
- [ ] Implement `OsaScript` struct
- [ ] Implement `OsaScript::escape_path()` helper
- [ ] Implement `OsaScript::build_file_script()` helper
- [ ] Implement `OsaScript::run_script()` helper
- [ ] Implement `CopyTool` trait for `OsaScript`
- [ ] Implement `Default` trait
- [ ] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/mod.rs`, `src/clipboard/tools/osascript.rs`

**Verify**: `cargo test clipboard::tools::osascript`

---

### Stage 5: Pbcopy Tool (tools/pbcopy.rs)

**Goal**: Implement macOS content copy via pbcopy.

#### Red Phase
- [ ] Create `src/clipboard/tools/pbcopy.rs` with struct stub
- [ ] Write test: `method()` returns `CopyMethod::Pbcopy`
- [ ] Write test: `is_available()` returns true on macOS cfg
- [ ] Write test: `can_copy_files()` returns false
- [ ] Write test: `try_copy_file()` returns `NotSupported`

#### Green Phase
- [ ] Implement `Pbcopy` struct
- [ ] Implement `CopyTool` trait for `Pbcopy`
- [ ] Implement `try_copy_text()` using stdin pipe to pbcopy
- [ ] Implement `Default` trait
- [ ] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/pbcopy.rs`, `src/clipboard/tools/mod.rs`

**Verify**: `cargo test clipboard::tools::pbcopy`

---

### Stage 6: Xclip Tool (tools/xclip.rs)

**Goal**: Implement Linux X11 clipboard via xclip.

#### Red Phase
- [ ] Create `src/clipboard/tools/xclip.rs` with struct stub
- [ ] Write test: `build_file_uri()` creates correct file:// URI
- [ ] Write test: `build_file_uri()` handles paths with spaces (URI encoding)
- [ ] Write test: `method()` returns `CopyMethod::Xclip`
- [ ] Write test: `is_available()` checks for xclip binary
- [ ] Write test: `can_copy_files()` returns true

#### Green Phase
- [ ] Implement `Xclip` struct
- [ ] Implement `Xclip::build_file_uri()` helper
- [ ] Implement `CopyTool` trait for `Xclip`
- [ ] Implement `try_copy_file()` using `-t text/uri-list`
- [ ] Implement `try_copy_text()` using `-selection clipboard`
- [ ] Implement `Default` trait
- [ ] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/xclip.rs`, `src/clipboard/tools/mod.rs`

**Verify**: `cargo test clipboard::tools::xclip`

---

### Stage 7: Xsel Tool (tools/xsel.rs)

**Goal**: Implement Linux X11 alternative via xsel.

#### Red Phase
- [ ] Create `src/clipboard/tools/xsel.rs` with struct stub
- [ ] Write test: `method()` returns `CopyMethod::Xsel`
- [ ] Write test: `is_available()` checks for xsel binary
- [ ] Write test: `can_copy_files()` returns false (xsel is text-only)
- [ ] Write test: `try_copy_file()` returns `NotSupported`

#### Green Phase
- [ ] Implement `Xsel` struct
- [ ] Implement `CopyTool` trait for `Xsel`
- [ ] Implement `try_copy_text()` using `--clipboard --input`
- [ ] Implement `Default` trait
- [ ] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/xsel.rs`, `src/clipboard/tools/mod.rs`

**Verify**: `cargo test clipboard::tools::xsel`

---

### Stage 8: WlCopy Tool (tools/wl_copy.rs)

**Goal**: Implement Linux Wayland clipboard via wl-copy.

#### Red Phase
- [ ] Create `src/clipboard/tools/wl_copy.rs` with struct stub
- [ ] Write test: `method()` returns `CopyMethod::WlCopy`
- [ ] Write test: `is_available()` checks for wl-copy binary
- [ ] Write test: `can_copy_files()` returns false (wl-copy is text-only for our use)
- [ ] Write test: `try_copy_file()` returns `NotSupported`

#### Green Phase
- [ ] Implement `WlCopy` struct
- [ ] Implement `CopyTool` trait for `WlCopy`
- [ ] Implement `try_copy_text()` using stdin pipe
- [ ] Implement `Default` trait
- [ ] Export in `src/clipboard/tools/mod.rs`

**Files**: `src/clipboard/tools/wl_copy.rs`, `src/clipboard/tools/mod.rs`

**Verify**: `cargo test clipboard::tools::wl_copy`

---

### Stage 9: Platform Tool Selection (tools/mod.rs)

**Goal**: Implement `platform_tools()` and `tool_exists()` helpers.

#### Red Phase
- [ ] Write test: `tool_exists()` returns false for nonexistent tool
- [ ] Write test: `platform_tools()` returns OsaScript, Pbcopy on macOS
- [ ] Write test: `platform_tools()` returns Xclip, Xsel, WlCopy on Linux
- [ ] Write test: `platform_tools()` returns empty vec on other platforms

#### Green Phase
- [ ] Implement `tool_exists()` using `which` command
- [ ] Implement `platform_tools()` with cfg attributes
- [ ] Wire up `Copy::new()` to use `platform_tools()`

**Files**: `src/clipboard/tools/mod.rs`, `src/clipboard/copy.rs`

**Verify**: `cargo test clipboard::tools`

---

### Stage 10: Public API (mod.rs)

**Goal**: Expose clean public API `copy_file_to_clipboard()`.

#### Red Phase
- [ ] Write test: `copy_file_to_clipboard()` returns error for non-existent file
- [ ] Write test: `copy_file_to_clipboard()` delegates to `Copy::new().file()`

#### Green Phase
- [ ] Implement `copy_file_to_clipboard()` function
- [ ] Ensure all public types are re-exported
- [ ] Add module documentation

**Files**: `src/clipboard/mod.rs`

**Verify**: `cargo test clipboard && cargo doc --no-deps`

---

### Stage 11: CLI Command Definition

**Goal**: Add `agr copy` command to CLI with shell completion support.

**Important**: The argument MUST be named `file` (not `recording`) to enable automatic shell completion detection. The existing `has_file_argument()` function in `src/shell/completions.rs` checks for `arg.get_id() == "file"`.

#### Red Phase
- [ ] Write test: `agr copy --help` parses successfully
- [ ] Write test: `agr copy session.cast` parses with correct file argument
- [ ] Write test: `agr copy` without args shows error
- [ ] Write test: `extract_commands()` includes `copy` command
- [ ] Write test: `copy` command has `accepts_file == true` (completion detection)

#### Green Phase
- [ ] Add `Copy { file: String }` variant to `Commands` enum in `src/cli.rs`
  - **Critical**: Name the argument `file`, not `recording`
- [ ] Add help text and examples
- [ ] Verify help displays correctly
- [ ] Verify `extract_commands()` detects `copy` as file-accepting

**Files**: `src/cli.rs`

**Verify**: `cargo test cli` + `cargo test shell::completions` + `cargo run -- copy --help`

---

### Stage 12: CLI Command Handler

**Goal**: Implement the copy command handler.

#### Red Phase
- [ ] Create `src/commands/copy.rs` with function stub
- [ ] Write test: handler returns error for non-existent file with helpful message
- [ ] Write test: handler accepts filename with and without .cast extension
- [ ] Write test: handler resolves short format paths (agent/file.cast)

#### Green Phase
- [ ] Implement `handle(file: &str) -> Result<()>`
  - Load config
  - Use `resolve_file_path()` for path resolution
  - Validate file exists with helpful error
  - Call `clipboard::copy_file_to_clipboard()`
  - Print themed result message
- [ ] Export module in `src/commands/mod.rs`

**Files**: `src/commands/copy.rs`, `src/commands/mod.rs`

**Verify**: `cargo test commands::copy`

---

### Stage 13: CLI Main Dispatch

**Goal**: Wire up command in main.rs.

#### Red Phase
- [ ] Write test: `Commands::Copy` variant exists and matches correctly

#### Green Phase
- [ ] Add match arm for `Commands::Copy { file }` in `main.rs`
- [ ] Call `commands::copy::handle(&file)`

**Files**: `src/main.rs`

**Verify**: `cargo build && cargo run -- copy --help`

---

### Stage 14: TUI Context Menu Item

**Goal**: Add Copy to context menu.

#### Red Phase
- [ ] Write test: `ContextMenuItem::Copy` exists
- [ ] Write test: `ContextMenuItem::Copy.label()` returns "Copy to clipboard"
- [ ] Write test: `ContextMenuItem::Copy.shortcut()` returns "c"
- [ ] Write test: `ContextMenuItem::ALL` has 6 items
- [ ] Write test: `ContextMenuItem::ALL[1]` is Copy (after Play)

#### Green Phase
- [ ] Add `Copy` variant to `ContextMenuItem` enum
- [ ] Update `ContextMenuItem::ALL` array (6 items, Copy at index 1)
- [ ] Implement `label()` match arm
- [ ] Implement `shortcut()` match arm

**Files**: `src/tui/list_app.rs`

**Verify**: `cargo test tui::list_app::context_menu`

---

### Stage 15: TUI Action Handler

**Goal**: Implement copy action in TUI.

#### Red Phase
- [ ] Write test: `c` key in Normal mode with selection triggers copy
- [ ] Write test: `c` key in ContextMenu mode selects Copy and executes
- [ ] Write test: copy action sets status message on success
- [ ] Write test: copy action sets error status message on failure

#### Green Phase
- [ ] Add `copy_to_clipboard(&mut self) -> Result<()>` method to `ListApp`
- [ ] Add `KeyCode::Char('c')` handling in `handle_normal_key()`
- [ ] Add `KeyCode::Char('c')` handling in `handle_context_menu_key()`
- [ ] Wire up `ContextMenuItem::Copy` in `execute_context_menu_action()`

**Files**: `src/tui/list_app.rs`

**Verify**: `cargo test tui::list_app`

---

### Stage 16: TUI Help Text

**Goal**: Document copy keybinding in TUI help.

#### Red Phase
- [ ] Write snapshot test: help modal includes "c" and "Copy" text

#### Green Phase
- [ ] Add copy keybinding line to `render_help_modal()` in Actions section
- [ ] Update `modal_height` calculation (now 27 lines)

**Files**: `src/tui/list_app.rs`

**Verify**: `cargo test snapshot` + visual verification

---

### Stage 17: README Documentation

**Goal**: Document copy feature for users.

- [ ] Add `agr copy` example to Quick Start section
- [ ] Add copy command documentation section with examples
- [ ] Document `c` keybinding in TUI help or controls section
- [ ] Add note about platform behavior (macOS file copy vs Linux content fallback)

**Files**: `README.md`

**Verify**: Manual review

---

### Stage 18: Generated Documentation

**Goal**: Update generated docs.

- [ ] Run `cargo xtask gen-docs`
- [ ] Verify copy command appears in `docs/COMMANDS.md`
- [ ] Verify wiki pages updated if applicable
- [ ] Review diff for accuracy

**Files**: `docs/COMMANDS.md`, `docs/wiki/*`

**Verify**: `cargo xtask gen-docs && git diff docs/`

---

### Stage 19: Shell Completion Verification

**Goal**: Verify shell completion detects and supports the `copy` command.

#### Red Phase
- [ ] Write test: `extract_commands()` includes "copy" in returned commands
- [ ] Write test: `copy` command has `accepts_file == true`
- [ ] Write test: generated zsh init contains "copy" in `_agr_file_cmds`
- [ ] Write test: generated bash init contains "copy" in `_agr_file_cmds`

#### Green Phase
- [ ] Verify existing tests pass (no code changes needed if arg named `file`)
- [ ] If tests fail, ensure CLI argument is named `file` not `recording`

**Files**: `src/shell/completions.rs` (tests only, no implementation changes)

**Verify**: `cargo test shell::completions`

---

### Stage 20: Integration Tests

**Goal**: End-to-end CLI verification.

- [ ] Create `tests/integration/copy_test.rs`
- [ ] Test: `agr copy --help` exits 0 and shows usage
- [ ] Test: `agr copy nonexistent.cast` exits non-zero with helpful error
- [ ] Test: `agr copy` without arguments shows error
- [ ] Test: path resolution works with short format
- [ ] Test: (cfg macos) copy succeeds with temp file
- [ ] Test: (cfg linux) copy succeeds or fails gracefully based on tools
- [ ] Test: `agr completions --files` includes test recordings

**Files**: `tests/integration/copy_test.rs`

**Verify**: `cargo test --test copy_test`

---

### Stage 21: Manual Platform Testing

**Goal**: Verify real-world behavior.

- [ ] **macOS**: Run `agr copy <file>`, paste into Slack, verify file attachment works
- [ ] **macOS**: Run `agr ls`, press `c`, paste into Slack
- [ ] **macOS**: Test `agr copy <TAB>` shows recording completions
- [ ] **Linux X11**: Test with xclip installed
- [ ] **Linux X11**: Test with only xsel installed
- [ ] **Linux Wayland**: Test with wl-copy installed
- [ ] **Linux no tools**: Verify helpful error message
- [ ] **Linux**: Test `agr copy <TAB>` shows recording completions

**Verify**: Manual testing on each platform

---

## Dependencies

```
Stage 1 (Types) ──> Stage 2 (Trait) ──> Stage 3 (Copy) ──────────────────────────────────┐
                                             │                                            │
                    ┌────────────────────────┼────────────────────────┐                   │
                    v                        v                        v                   │
              Stage 4 (osascript)    Stage 6 (xclip)           Stage 8 (wl-copy)         │
                    │                        │                        │                   │
                    v                        v                        v                   │
              Stage 5 (pbcopy)       Stage 7 (xsel)                   │                   │
                    │                        │                        │                   │
                    └────────────────────────┴────────────────────────┘                   │
                                             │                                            │
                                             v                                            │
                                      Stage 9 (platform_tools) ──> Stage 10 (Public API) ─┘
                                                                          │
                              ┌───────────────────────────────────────────┤
                              │                                           │
                              v                                           v
                       Stage 11 (CLI Def)                         Stage 14 (TUI Menu)
                              │                                           │
                              v                                           v
                       Stage 12 (CLI Handler)                     Stage 15 (TUI Action)
                              │                                           │
                              v                                           v
                       Stage 13 (Main Dispatch)                   Stage 16 (TUI Help)
                              │                                           │
                              └─────────────────┬─────────────────────────┘
                                                │
                                                v
                                         Stage 17 (README)
                                                │
                                                v
                                         Stage 18 (Gen Docs)
                                                │
                                                v
                                         Stage 19 (Completions) ──> Stage 20 (Integration)
                                                                          │
                                                                          v
                                                                   Stage 21 (Manual Test)
```

**Parallelization opportunities:**
- Stages 4-5 (macOS tools) can run in parallel with Stages 6-8 (Linux tools)
- Stages 11-13 (CLI) can run in parallel with Stages 14-16 (TUI) after Stage 10
- Stage 19 (Completions) can run in parallel with Stage 17-18 after Stage 13

---

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | pending | Core types |
| 2 | pending | CopyTool trait |
| 3 | pending | Copy orchestrator |
| 4 | pending | OsaScript tool |
| 5 | pending | Pbcopy tool |
| 6 | pending | Xclip tool |
| 7 | pending | Xsel tool |
| 8 | pending | WlCopy tool |
| 9 | pending | Platform selection |
| 10 | pending | Public API |
| 11 | pending | CLI definition (arg must be named `file` for completions) |
| 12 | pending | CLI handler |
| 13 | pending | Main dispatch |
| 14 | pending | TUI menu item |
| 15 | pending | TUI action handler |
| 16 | pending | TUI help text |
| 17 | pending | README docs |
| 18 | pending | Generated docs |
| 19 | pending | Shell completion verification |
| 20 | pending | Integration tests |
| 21 | pending | Manual testing |

---

## Test Commands

```bash
# Run all tests
cargo test

# Run specific module tests
cargo test clipboard::result
cargo test clipboard::error
cargo test clipboard::tool
cargo test clipboard::copy
cargo test clipboard::tools::osascript
cargo test clipboard::tools::pbcopy
cargo test clipboard::tools::xclip
cargo test clipboard::tools::xsel
cargo test clipboard::tools::wl_copy
cargo test clipboard::tools

# Run CLI tests
cargo test cli

# Run TUI tests
cargo test tui::list_app

# Run shell completion tests
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
