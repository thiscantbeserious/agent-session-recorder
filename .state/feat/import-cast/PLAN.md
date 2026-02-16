# Plan: Drop/Paste .cast Files into TUI

References: ADR.md

## Open Questions

None -- all resolved in ADR.

---

## Stages

### Stage 1: Enable Bracketed Paste and Event Plumbing

Goal: Add `Event::Paste(String)` to the event bus and enable/disable bracketed paste in the terminal lifecycle. After this stage, paste events reach the TUI app event loop.

Owner: implementer
Files: `src/tui/event_bus.rs`, `src/tui/app/mod.rs`
Depends on: none

#### Changes

**`src/tui/event_bus.rs` -- Event enum:**
- [x] Add `Paste(String)` variant to `Event` enum

**`src/tui/event_bus.rs` -- EventHandler thread:**
- [x] Add `Ok(CrosstermEvent::Paste(text))` arm in the event read match
- [x] Forward as `Event::Paste(text)` via the mpsc channel

**`src/tui/app/mod.rs` -- App::new():**
- [x] Add `EnableBracketedPaste` to the `execute!` call (after `EnableMouseCapture`)
- [x] Add import: `use crossterm::event::{EnableBracketedPaste, DisableBracketedPaste};`

**`src/tui/app/mod.rs` -- App::drop():**
- [x] Add `DisableBracketedPaste` to the `execute!` call (after `DisableMouseCapture`)

**`src/tui/app/mod.rs` -- App::suspend():**
- [x] Add `DisableBracketedPaste` to the `execute!` call (after `DisableMouseCapture`)

**`src/tui/app/mod.rs` -- App::resume():**
- [x] Add `EnableBracketedPaste` to the `execute!` call (after `EnableMouseCapture`)

**`src/tui/app/mod.rs` -- TuiApp::run():**
- [x] Add `Event::Paste(text)` arm in the event loop match -- call `self.handle_paste(text)?`
- [x] Add `fn handle_paste(&mut self, _text: String) -> Result<()> { Ok(()) }` default method on `TuiApp` trait

**Tests:**
- [x] `event_paste_variant_debug` -- verify `Event::Paste("foo".into())` debug format
- [x] `event_paste_clone` -- verify clone works for Paste variant

**Verify:** `cargo test tui::event_bus && cargo clippy -- -D warnings`

---

### Stage 2: StorageManager Import API

Goal: Add `import_cast_file()` method to `StorageManager` that copies a validated `.cast` file into managed storage with conflict resolution. Add `validate_cast_header()` as a standalone function.

Owner: implementer
Files: `src/storage.rs`
Depends on: none

#### Changes

**`src/storage.rs` -- New `validate_cast_header()` function:**
- [x] Signature: `pub fn validate_cast_header(path: &Path) -> Result<(), ImportError>`
- [x] Check file exists and is readable (map IO errors to `ImportError::NotFound` / `ImportError::PermissionDenied`)
- [x] Check `.cast` extension (`ImportError::WrongExtension`)
- [x] Read first line only via `BufReader::new(File::open(path)?).lines().next()`
- [x] Parse first line as `serde_json::Value`, check for `"version"` key (`ImportError::InvalidFormat`)
- [x] Accept version 2 or 3 (the requirements say "asciicast v2" but the codebase uses v3; accept both)

**`src/storage.rs` -- New `ImportError` enum:**
- [x] Variants: `NotFound(String)`, `PermissionDenied(String)`, `WrongExtension(String)`, `InvalidFormat(String)`, `CopyFailed(String)`
- [x] Derive `Debug, Clone`, implement `Display` and `std::error::Error`

**`src/storage.rs` -- New `StorageManager::import_cast_file()` method:**
- [x] Signature: `pub fn import_cast_file(&self, source: &Path, agent: &str) -> Result<PathBuf, ImportError>`
- [x] Call `validate_cast_header(source)?`
- [x] Call `self.ensure_agent_dir(agent)` (map error to `ImportError::CopyFailed`)
- [x] Determine target filename: use source filename, resolve conflicts:
  - If `session.cast` exists, try `session-1.cast`, `session-2.cast`, etc.
  - Helper: `fn resolve_filename_conflict(dir: &Path, filename: &str) -> PathBuf`
- [x] Call `fs::copy(source, &target)` (map error to `ImportError::CopyFailed`)
- [x] Return `Ok(target)`

**`src/storage.rs` -- New `resolve_filename_conflict()` helper:**
- [x] Signature: `fn resolve_filename_conflict(dir: &Path, filename: &str) -> PathBuf`
- [x] If `dir/filename` does not exist, return it
- [x] Otherwise split filename into stem and extension, try `stem-1.ext`, `stem-2.ext`, up to 999
- [x] Return the first non-existing path

**Tests:**
- [x] `validate_cast_header_valid_v3` -- valid v3 file passes
- [x] `validate_cast_header_valid_v2` -- valid v2 file passes
- [x] `validate_cast_header_missing_file` -- returns NotFound
- [x] `validate_cast_header_wrong_extension` -- returns WrongExtension for `.txt`
- [x] `validate_cast_header_invalid_json` -- returns InvalidFormat
- [x] `validate_cast_header_no_version` -- returns InvalidFormat for JSON without version
- [x] `import_cast_file_success` -- copies file to correct agent dir (tempfile)
- [x] `import_cast_file_creates_agent_dir` -- creates non-existing agent dir
- [x] `import_cast_file_conflict_resolution` -- second import gets `-1` suffix
- [x] `import_cast_file_preserves_filename` -- original filename used when no conflict
- [x] `resolve_filename_conflict_no_conflict` -- returns original path
- [x] `resolve_filename_conflict_with_existing` -- returns `-1` suffixed path

**Verify:** `cargo test storage && cargo clippy -- -D warnings`

---

### Stage 3: Import State and Path Parsing

Goal: Create `src/tui/import.rs` with `ImportState`, `ImportPhase`, path parsing, and agent autocomplete logic. No rendering or TUI wiring yet -- purely state and logic.

Owner: implementer
Files: `src/tui/import.rs`, `src/tui/mod.rs`
Depends on: none

#### Changes

**New file `src/tui/import.rs`:**

**Path parsing:**
- [x] `pub fn parse_paste_paths(text: &str) -> Vec<PathBuf>`
  - Split on newlines
  - Trim each line, skip empty lines
  - Trim surrounding single/double quotes from each path
  - Expand tilde (`~`) to home directory via `dirs::home_dir()`
  - Resolve relative paths against `std::env::current_dir()`
  - Collect into `Vec<PathBuf>`

**ImportPhase enum:**
- [x] `AgentInput` -- user is typing agent name
- [x] `Importing` -- validation and copy in progress
- [x] `Done` -- results ready for display

**ImportResult struct:**
- [x] `filename: String` -- original filename
- [x] `outcome: Result<PathBuf, String>` -- destination path on success, error message on failure

**ImportState struct:**
- [x] `phase: ImportPhase`
- [x] `paths: Vec<PathBuf>` -- parsed file paths
- [x] `agent_input: String` -- current agent name input
- [x] `agent_cursor: usize` -- cursor byte offset
- [x] `filtered_agents: Vec<String>` -- agents matching prefix
- [x] `autocomplete_idx: Option<usize>` -- selected suggestion index
- [x] `results: Vec<ImportResult>` -- import results

**ImportState methods:**
- [x] `pub fn new(paste_text: &str, available_agents: &[String]) -> Self`
  - Parse paths from paste text
  - Initialize agent input empty, filter all agents
  - Start in `AgentInput` phase
- [x] `pub fn update_agent_filter(&mut self, available_agents: &[String])`
  - Filter available_agents (skip "All") by prefix match on `agent_input`
  - Reset `autocomplete_idx` to `Some(0)` if matches exist, else `None`
- [x] `pub fn autocomplete_up(&mut self)` -- cycle autocomplete selection up
- [x] `pub fn autocomplete_down(&mut self)` -- cycle autocomplete selection down
- [x] `pub fn accept_autocomplete(&mut self)` -- fill `agent_input` from selected suggestion
- [x] `pub fn selected_agent(&self) -> &str` -- returns `agent_input` (trimmed)
- [x] `pub fn file_count(&self) -> usize` -- returns `paths.len()`
- [x] `pub fn has_paths(&self) -> bool` -- returns `!paths.is_empty()`
- [x] `pub fn agent_input_char(&mut self, c: char)` -- insert char at cursor
- [x] `pub fn agent_input_backspace(&mut self)` -- delete char before cursor
- [x] `pub fn agent_input_delete(&mut self)` -- delete char at cursor
- [x] `pub fn agent_input_left(&mut self)` -- move cursor left
- [x] `pub fn agent_input_right(&mut self)` -- move cursor right

**`src/tui/mod.rs`:**
- [x] Add `pub mod import;` declaration

**Tests (in `src/tui/import.rs` or `tests/`):**
- [x] `parse_paste_single_absolute_path`
- [x] `parse_paste_multiple_paths_newline_separated`
- [x] `parse_paste_tilde_expansion`
- [x] `parse_paste_quoted_paths`
- [x] `parse_paste_empty_lines_skipped`
- [x] `parse_paste_relative_path_resolved`
- [x] `import_state_new_parses_paths`
- [x] `import_state_agent_filter_prefix_match`
- [x] `import_state_autocomplete_cycle`
- [x] `import_state_accept_autocomplete_fills_input`
- [x] `import_state_agent_input_char_insert`
- [x] `import_state_agent_input_backspace`

**Verify:** `cargo test tui::import && cargo clippy -- -D warnings`

---

### Stage 4: Import Mode TUI Wiring and Key Handling

Goal: Add `Mode::Import` to `list_app.rs`, wire paste events to create `ImportState`, handle keys in Import mode, and trigger the actual import via `StorageManager`. Thin delegation only -- all logic lives in `import.rs` and `storage.rs`.

Owner: implementer
Files: `src/tui/list_app.rs`
Depends on: Stage 1, Stage 2, Stage 3

#### Changes

**`src/tui/list_app.rs` -- Mode enum:**
- [x] Add `Import` variant
- [x] Add arm to `Mode::to_shared()`: `Mode::Import => None`

**`src/tui/list_app.rs` -- ListApp struct:**
- [x] Add `import_state: Option<import::ImportState>` field
- [x] Initialize as `None` in `ListApp::new()`

**`src/tui/list_app.rs` -- Paste handling:**
- [x] Override `handle_paste()` from `TuiApp` trait
- [x] Create `ImportState::new(text, &self.shared.available_agents)`
- [x] If `import_state.has_paths()` is false, set status message "No file paths found in pasted text" and return
- [x] Set `self.import_state = Some(state)` and `self.mode = Mode::Import`

**`src/tui/list_app.rs` -- New `handle_import_key()` method:**
- [x] Guard: `let state = self.import_state.as_mut().unwrap()` (safe: only called in Import mode)
- [x] Match on `state.phase`:
  - `AgentInput`:
    - `Esc` -> cancel import, set mode Normal, clear import_state
    - `Enter` -> if agent_input is non-empty, transition to Importing phase, call `self.execute_import()`
    - `Up` -> `state.autocomplete_up()`
    - `Down` -> `state.autocomplete_down()`
    - `Tab` -> `state.accept_autocomplete()`, update filter
    - `Backspace` -> `state.agent_input_backspace()`, update filter
    - `Char(c)` -> `state.agent_input_char(c)`, update filter
  - `Importing` -> ignore all keys (brief synchronous phase)
  - `Done`:
    - `Enter` or `Esc` -> set mode Normal, clear import_state

**`src/tui/list_app.rs` -- New `execute_import()` method:**
- [x] Get storage manager from `self.shared.storage`
- [x] Get agent name from `import_state.selected_agent()`
- [x] For each path in `import_state.paths`:
  - Call `storage.import_cast_file(&path, agent)`
  - Record result in `import_state.results`
- [x] Transition `import_state.phase` to `Done`

**`src/tui/list_app.rs` -- `handle_key()` dispatch:**
- [x] Add `Mode::Import => self.handle_import_key(key)?` arm

**`src/tui/list_app.rs` -- Mouse handling:**
- [x] Add `Mode::Import` arm in `handle_mouse()`: click outside modal cancels (like other modals)

**`src/tui/list_app.rs` -- Help modal:**
- [x] Add line: "Paste: Import .cast file(s)" in the Actions section

**`src/tui/list_app.rs` -- Tests:**
- [x] `import_mode_exists` -- mode variant equality
- [x] `paste_with_no_paths_shows_status` -- paste empty text stays in Normal mode

**Verify:** `cargo test tui::list_app && cargo clippy -- -D warnings`

---

### Stage 5: Import Modal Rendering

Goal: Add rendering methods to `ImportState` for the centered modal overlay. Wire into `ListApp::draw()`.

Owner: implementer
Files: `src/tui/import.rs`, `src/tui/list_app.rs`
Depends on: Stage 3, Stage 4

#### Changes

**`src/tui/import.rs` -- Rendering methods:**
- [x] `pub fn render(state: &ImportState, frame: &mut Frame, area: Rect)`
  - Dispatch to phase-specific render function
- [x] `fn render_agent_input(state: &ImportState, frame: &mut Frame, area: Rect)`
  - Centered modal (~50 wide, ~12 tall)
  - Title: " Import {n} file(s) "
  - Agent input line with cursor
  - Autocomplete dropdown (filtered agents list, highlighted selection)
  - Footer: "Enter: confirm | Esc: cancel | Tab: complete"
- [x] `fn render_result(state: &ImportState, frame: &mut Frame, area: Rect)`
  - Centered modal (sized to fit results)
  - Title: " Import Complete " or " Import Results "
  - Per-file status: checkmark for success, X for failure with error
  - Footer: "Enter/Esc: dismiss"

**`src/tui/list_app.rs` -- draw() method:**
- [x] Add `Mode::Import` arm in modal overlay rendering section:
  - `if let Some(ref state) = import_state { import::ImportState::render(state, frame, area); }`
- [x] Add `Mode::Import` arm in status_text match:
  - AgentInput: show "Import: select agent for {n} file(s)"
  - Done: show summary (e.g., "Imported 2/3 files")
- [x] Add `Mode::Import` arm in footer_text match:
  - AgentInput: "Enter: confirm | Esc: cancel | Tab: complete | Up/Down: select"
  - Done: "Enter/Esc: dismiss"

**Tests:**
- [x] Snapshot test: `import_agent_input_modal` -- render AgentInput phase modal
- [x] Snapshot test: `import_result_modal_success` -- render Done phase with all successes
- [x] Snapshot test: `import_result_modal_mixed` -- render Done phase with mixed results

**Verify:** `cargo test tui::import && cargo test --test snapshot_tui_test && cargo clippy -- -D warnings`

---

### Stage 6: Integration Tests and Documentation

Goal: Add integration tests for the full paste-to-import flow and update documentation.

Owner: implementer
Files: `tests/integration/import_test.rs`, `src/tui/list_app.rs`, `src/storage.rs`
Depends on: Stage 4, Stage 5

#### Changes

**`tests/integration/` -- Integration tests:**
- [x] `import_test.rs` -- test `StorageManager::import_cast_file()` end-to-end with tempdir
- [x] Test import with conflict resolution (import same file twice)
- [x] Test import with invalid file (not a .cast)
- [x] Test import with non-existent file
- [x] Test `parse_paste_paths()` with realistic paste content

**Documentation:**
- [x] Update module doc comment in `src/tui/list_app.rs`: add "import" to feature list
- [x] Update module doc comment in `src/storage.rs`: mention import capability

**Verify:** `cargo test && cargo clippy -- -D warnings`

---

## Dependencies

```
Stage 1 (Paste Events)  ─────────────────────┐
                                              │
Stage 2 (Storage Import API) ─────────────────┤
                                              ├──> Stage 4 (TUI Wiring) ──> Stage 5 (Rendering) ──> Stage 6 (Tests + Docs)
Stage 3 (Import State + Path Parsing) ────────┘
```

**Parallel execution**: Stages 1, 2, and 3 have **no overlapping file ownership** and can execute in parallel.

| Stage | Files Owned | Can Parallel With |
|-------|------------|-------------------|
| 1 | `src/tui/event_bus.rs`, `src/tui/app/mod.rs` | 2, 3 |
| 2 | `src/storage.rs` | 1, 3 |
| 3 | `src/tui/import.rs` (new), `src/tui/mod.rs` | 1, 2 |
| 4 | `src/tui/list_app.rs` | -- (depends on 1, 2, 3) |
| 5 | `src/tui/import.rs`, `src/tui/list_app.rs` | -- (depends on 3, 4) |
| 6 | `tests/integration/import_test.rs` (new) | -- (depends on 4, 5) |

---

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | complete | Paste events plumbing |
| 2 | complete | Storage import API |
| 3 | complete | Import state + path parsing |
| 4 | complete | TUI wiring |
| 5 | complete | Modal rendering |
| 6 | complete | Integration tests + docs |

---

## Test Commands

```bash
# Run event bus tests
cargo test tui::event_bus

# Run storage tests
cargo test storage

# Run import module tests
cargo test tui::import

# Run list_app tests
cargo test tui::list_app

# Run snapshot tests
cargo test --test snapshot_tui_test

# Run integration tests
cargo test --test integration

# Full test suite
cargo test

# Check for warnings/lints
cargo clippy -- -D warnings

# Format check
cargo fmt -- --check
```
