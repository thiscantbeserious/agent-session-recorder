# ADR: Drop/Paste .cast Files into TUI

## Status
Accepted

## Context

The `agr ls` TUI provides an interactive file explorer for managing session recordings stored in `~/recorded_agent_sessions/{agent}/`. Users currently have no way to import existing `.cast` files from external sources (manual asciinema sessions, shared recordings, archived sessions) without manually copying files via shell commands. This breaks the TUI-first workflow that AGR promotes.

### Technical Context

The codebase already has:
- `Mode` enum state machine in `list_app.rs` with modes like `Normal`, `Search`, `AgentFilter`, `RenameInput`, `ContextMenu`, etc.
- `StorageManager::ensure_agent_dir(agent)` for creating agent directories
- `StorageManager::list_sessions()` for scanning `~/recorded_agent_sessions/{agent}/*.cast`
- `AsciicastFile::parse(path)` for full v3 validation (header + all events)
- `SharedState::available_agents` for the agent list used by agent filter
- 3-second tick refresh via `maybe_refresh_tick()` that rescans the filesystem
- `EventHandler` thread that polls crossterm events and forwards them via mpsc channel
- crossterm 0.28 which includes `EnableBracketedPaste`/`DisableBracketedPaste` and `Event::Paste` in its default feature set (no Cargo.toml changes needed)
- `RenameInput` mode with inline text input as the established pattern for text input in the TUI
- Agent filter cycling with `available_agents` list (first entry is "All", rest are agent names)

### Requirements Summary

1. Enable bracketed paste in terminal when TUI starts
2. Detect `Event::Paste` events containing file paths
3. Enter Import mode with agent selection (autocomplete from existing agents)
4. Validate `.cast` files (existence, extension, asciicast v2/v3 header)
5. Copy files into managed storage under chosen agent directory
6. Handle filename conflicts with counter suffix
7. Show success/failure feedback, return to Normal mode

## Options Considered

### Option A: Monolithic Import Mode in list_app.rs

Add `Mode::Import` with sub-states tracked via a struct field (e.g., `ImportPhase::AgentSelect`, `ImportPhase::Validating`, `ImportPhase::Result`). All import logic lives directly in `list_app.rs` methods.

**Pros:**
- Direct access to `SharedState`, `App`, and `explorer` without indirection
- Follows the pattern of existing modes (all in one file)

**Cons:**
- `list_app.rs` is already 1668 lines (well above the 400-line target)
- Import logic (path parsing, validation, copying) is domain logic, not TUI logic
- Adds 200+ lines of import-specific state, handlers, and rendering to an already large file
- Makes testing harder; import logic cannot be tested without TUI state

### Option B: Dedicated Import Module with Thin TUI Integration

Create `src/tui/import.rs` (~200 lines) containing:
- `ImportState` struct with phase tracking, agent input, file list, results
- All import-specific key handling methods
- All import-specific rendering methods
- Path parsing and validation logic

`list_app.rs` gets:
- `Mode::Import` variant
- `import_state: Option<ImportState>` field
- Thin delegation in `handle_key()` and `draw()` to `ImportState` methods

File validation and storage copy operations go into `src/storage.rs` as `StorageManager` methods.

**Pros:**
- Keeps `list_app.rs` growth minimal (~20 lines of delegation code)
- Import logic is self-contained and independently testable
- `StorageManager` is the natural home for import/copy operations
- Follows coding principles: single responsibility, ~400 line files, ~20 line functions

**Cons:**
- New module to create and maintain
- Import rendering needs to accept `Frame` and `Rect` from the caller (minor indirection)
- `ImportState` needs access to `available_agents` from `SharedState` (passed as parameter)

### Option C: Separate Import Module with Domain Logic Split

Like Option B, but additionally extract all file validation and path parsing into a standalone `src/import.rs` module (not under `tui/`). The TUI module handles only UI state and rendering.

**Pros:**
- Maximum separation of concerns
- Validation logic reusable by a future CLI import command

**Cons:**
- Three modules for one feature (overkill for current scope)
- Import is TUI-only per requirements; future CLI reuse is speculative
- More indirection for something that can be cleanly two modules

## Decision

**Option B: Dedicated Import Module with Thin TUI Integration**

Rationale:
1. **File size discipline**: `list_app.rs` at 1668 lines already needs no additional bulk. A dedicated `src/tui/import.rs` keeps import logic self-contained (~200-250 lines).
2. **Testability**: Import state transitions, path parsing, validation, and agent autocomplete filtering can all be unit tested without constructing a full `ListApp`.
3. **Natural StorageManager fit**: Copy-to-managed-storage is a storage operation. Adding `import_cast_file()` to `StorageManager` follows the existing pattern where `ensure_agent_dir()`, `list_sessions()`, and `resolve_cast_path()` already live.
4. **Minimal list_app changes**: `Mode::Import` variant, `import_state: Option<ImportState>` field, and thin delegation in `handle_key()`/`draw()` (~20 lines total).

### Design Details

#### Bracketed Paste Plumbing

- **Enable**: Add `EnableBracketedPaste` to the `execute!` call in `App::new()` (alongside `EnterAlternateScreen`, `EnableMouseCapture`)
- **Disable**: Add `DisableBracketedPaste` to `App::drop()` and `App::suspend()` (alongside `LeaveAlternateScreen`, `DisableMouseCapture`)
- **Re-enable**: Add `EnableBracketedPaste` to `App::resume()` (alongside `EnterAlternateScreen`, `EnableMouseCapture`)
- **Event forwarding**: Add `CrosstermEvent::Paste(text)` arm in `EventHandler` thread, forwarded as new `Event::Paste(String)` variant

#### Import State Machine

```
Normal --[Event::Paste(text)]--> Import(AgentInput)
                                   |
                          [Enter with valid agent]
                                   |
                                   v
                            Import(Processing)
                                   |
                          [validation + copy done]
                                   |
                                   v
                            Import(Result)
                                   |
                          [Enter or Esc]
                                   |
                                   v
                                Normal
```

`ImportState` struct:
- `phase: ImportPhase` (AgentInput, Processing, Result)
- `paths: Vec<PathBuf>` -- parsed from paste text
- `agent_input: String` -- text input for agent name
- `agent_cursor: usize` -- cursor position in agent input
- `filtered_agents: Vec<String>` -- agents matching current input (for autocomplete)
- `autocomplete_idx: Option<usize>` -- selected autocomplete suggestion
- `results: Vec<ImportResult>` -- per-file success/error after processing

#### Agent Autocomplete

- Filters `available_agents` (excluding "All") by prefix match on `agent_input`
- Up/Down arrows cycle through `filtered_agents`
- Tab or Right arrow completes the selected suggestion into `agent_input`
- Enter with empty autocomplete list creates a new agent directory
- Rendered as a dropdown-style list below the agent input line

#### File Validation (Lightweight)

Requirements say "Parse first line to validate asciicast v2 format (must be valid JSON with version field)." The codebase parser (`AsciicastFile::parse`) validates v3 only and reads the entire file. For import, we need a lightweight header-only check:

- New function `validate_cast_header(path: &Path) -> Result<(), ImportError>`:
  1. Check file exists and is readable
  2. Check `.cast` extension
  3. Read first line only
  4. Parse as JSON, check for `"version"` field (accept v2 or v3)
- Lives in `src/tui/import.rs` (import-specific, not general asciicast parsing)

#### StorageManager Import API

New method on `StorageManager`:
```rust
pub fn import_cast_file(&self, source: &Path, agent: &str) -> Result<PathBuf>
```
- Calls `ensure_agent_dir(agent)`
- Determines target filename (preserve original, resolve conflicts with `-1`, `-2` suffix)
- Copies file contents via `fs::copy()`
- Returns the destination path

#### Modal Rendering

The import modal is a centered overlay (like ConfirmDelete/OptimizeResult modals):
- **AgentInput phase**: Shows file count, agent input with autocomplete dropdown, instructions
- **Processing phase**: Shows "Importing..." (synchronous, so brief)
- **Result phase**: Shows per-file success/error summary, dismiss instructions

Rendered by `ImportState::render()` which accepts `&mut Frame, Rect` -- called from `ListApp::draw()`.

#### Path Parsing from Paste Text

- Split paste text on newlines and whitespace
- For each token: expand tilde (`~`), resolve relative paths against CWD
- Trim surrounding quotes (single or double) to handle terminal path quoting
- Collect into `Vec<PathBuf>`

## Consequences

### What becomes easier
- Importing external `.cast` files into managed storage without leaving the TUI
- Discovering import via natural paste gesture (no command to remember)
- Agent name entry with autocomplete reduces typos

### What becomes harder
- Nothing significant; the feature is additive

### Technical debt considerations
- New `Event::Paste(String)` variant in event bus (small, permanent addition)
- `EnableBracketedPaste` in terminal setup (permanent, but correct default for any paste-aware TUI)
- `import_state: Option<ImportState>` on `ListApp` (small footprint; `None` in normal operation)
- New `src/tui/import.rs` module (~200-250 lines)
- One new method on `StorageManager` (`import_cast_file`)

### Risks
- **Terminal compatibility**: Bracketed paste is widely supported (xterm, iTerm2, Terminal.app, GNOME Terminal, Windows Terminal) but some older terminals ignore it. Degradation is graceful -- without bracketed paste, the paste text arrives as individual key events (which are handled normally by the TUI, just not triggering import mode).
- **Large pastes**: A paste containing many paths could be slow to validate/copy. Acceptable for TUI single-user tool; if needed later, can move to async.
- **Path parsing edge cases**: Paths with spaces, quotes, or special characters require careful parsing. The design handles these explicitly (trim quotes, support spaces in quoted paths, tilde expansion).

## Decision History

1. Architect decided: Option B (dedicated import module) over monolithic (Option A) or over-split (Option C)
2. Architect decided: Lightweight header-only validation rather than full `AsciicastFile::parse`
3. Architect decided: `import_cast_file()` on `StorageManager` for copy logic
4. Architect decided: Import modal rendered by `ImportState::render()` for self-containment
5. Architect decided: Bracketed paste enable/disable in `App::new()`/`drop()`/`suspend()`/`resume()`
