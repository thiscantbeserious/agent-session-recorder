# Requirements: Drop/Paste .cast Files into TUI

## Problem Statement

Users currently have no way to import existing .cast files from external sources into AGR's managed storage (`~/recorded_agent_sessions/`). When users have recordings from manual asciinema sessions, shared recordings from colleagues, or archived sessions from other machines, they must manually copy files using shell commands and navigate the agent-specific directory structure. This creates friction and breaks the TUI-first workflow that AGR promotes.

## Desired Outcome

Users can paste file paths directly into the AGR list TUI and have those .cast files automatically imported into the managed storage under a chosen agent name. The import process should:
- Validate that files are legitimate asciicast v2 recordings
- Copy (not move or symlink) files into the appropriate agent directory
- Prompt for agent name with autocomplete from existing agents
- Show clear success/failure feedback
- Leverage the existing 3-second refresh mechanism to auto-display imported files

After import, the TUI shows the newly imported files in the list, and users can immediately interact with them using normal TUI operations (play, analyze, optimize, etc.).

## Scope

### In Scope

**Paste Detection & Handling:**
- Enable bracketed paste mode in the terminal when TUI starts
- Detect `Event::Paste` events from crossterm
- Parse pasted content to extract file path(s)
- Handle both single and multiple file paths in one paste
- Support absolute paths, relative paths, and tilde expansion (`~/file.cast`)
- Handle paths with spaces correctly

**Import Mode UI Flow:**
- New `Import` variant in TUI `Mode` enum
- Enter Import mode automatically when paste is detected
- Modal-style prompt asking for agent name
- Text input field with autocomplete showing existing agent names
- Option to create new agent folder if name doesn't exist
- Display validation progress (checking file format)
- Show success/failure status with details (filename imported, destination path)
- Return to Normal mode after import completes or is cancelled

**Agent Selection:**
- Autocomplete from existing agent directories in `~/recorded_agent_sessions/`
- Allow user to type new agent name to create new directory
- Use up/down arrows to navigate autocomplete suggestions
- Enter to confirm, Escape to cancel import

**File Validation:**
- Check that file path exists and is readable
- Verify file has `.cast` extension
- Parse first line to validate asciicast v2 format (must be valid JSON with "version" field)
- Reject non-cast files with clear error message

**Storage Operations:**
- Use `StorageManager::ensure_agent_dir()` to create target directory
- Copy file contents into `~/recorded_agent_sessions/{agent}/{filename}.cast`
- Handle filename conflicts by appending counter: `session.cast`, `session-1.cast`, `session-2.cast`
- Preserve original filename where possible

**Error Handling:**
- File not found: "File does not exist: {path}"
- Not a .cast file: "File must have .cast extension: {path}"
- Invalid format: "Not a valid asciicast recording: {path}"
- Permission errors: "Cannot read file: {permission error}"
- Write errors: "Failed to import to {destination}: {error}"
- Show errors in modal with option to retry or cancel

**User Feedback:**
- Progress indicator while validating and copying
- Success message: "Imported {filename} to {agent}/"
- Error details displayed in modal
- Clear instructions in modal footer (e.g., "Enter: confirm | Esc: cancel")

### Out of Scope

- CLI command for import (TUI only in this iteration)
- Drag-and-drop support (paste only)
- Batch import UI (though multiple paths in one paste is supported)
- Import from URLs or remote sources
- Automatic detection of agent name from file metadata
- Preview of .cast file before import
- Import history or undo functionality
- Moving or symlinking files (copy only)
- Validation beyond basic asciicast v2 format (won't check event integrity)

## Acceptance Criteria

- [ ] Bracketed paste is enabled when list TUI starts
- [ ] Pasting a file path into TUI enters Import mode
- [ ] Import mode displays agent selection prompt with autocomplete
- [ ] Autocomplete lists existing agent directories from `~/recorded_agent_sessions/`
- [ ] User can type a new agent name to create a new directory
- [ ] User can navigate autocomplete with up/down arrows, confirm with Enter
- [ ] Escape cancels import and returns to Normal mode
- [ ] File validation rejects files that don't exist, lack .cast extension, or aren't valid asciicast v2
- [ ] Valid .cast file is copied (not moved) to `~/recorded_agent_sessions/{agent}/`
- [ ] Filename conflicts are resolved by appending `-1`, `-2`, etc.
- [ ] Success message shows imported filename and destination agent
- [ ] Import errors display clear error message with option to retry or cancel
- [ ] After successful import, TUI refreshes and shows the new file in the list (within 3 seconds)
- [ ] Pasting multiple paths (newline or space-separated) imports all valid files
- [ ] Paths with spaces are handled correctly
- [ ] Tilde expansion (`~/file.cast`) works correctly
- [ ] Relative paths are resolved relative to current working directory

## Constraints

- Must integrate with existing TUI Mode state machine (add Import mode)
- Must use existing `StorageManager` for directory operations
- Must follow 3-second refresh pattern for auto-detection of new files
- Must use crossterm's bracketed paste support (`EnableBracketedPaste`, `DisableBracketedPaste`, `Event::Paste`)
- Must validate asciicast v2 format (first line is JSON with "version" field)
- Import is TUI-only for this iteration (no CLI command)
- No external dependencies beyond what's already in Cargo.toml

## Context

**Related Files:**
- `src/tui/list_app.rs` - TUI Mode enum, event handling
- `src/storage.rs` - StorageManager with `ensure_agent_dir()`, `list_sessions()`
- `src/asciicast.rs` - Parsing logic (may be used for validation)

**Architecture Notes:**
- TUI already has mode-based state machine (Normal, Search, RenameInput, etc.)
- StorageManager scans `~/recorded_agent_sessions/{agent}/*.cast`
- 3-second tick in TUI automatically refreshes file list
- No bracketed paste support exists today - needs to be added

**User Workflow:**
1. User opens AGR list TUI (`agr list`)
2. User pastes file path(s) from clipboard (Cmd+V or Ctrl+Shift+V)
3. TUI detects paste, enters Import mode
4. TUI prompts for agent name with autocomplete
5. User selects or types agent name, presses Enter
6. TUI validates file, copies to managed storage
7. TUI shows success message, returns to Normal mode
8. Within 3 seconds, TUI refreshes and displays imported file

**Design Philosophy:**
- Copy files (preserve originals) rather than move or symlink
- Fail fast with clear error messages rather than silent failures
- Leverage existing TUI patterns (modals, input fields, autocomplete)
- Keep UX consistent with other TUI operations (Escape to cancel, Enter to confirm)

---
**Sign-off:** Pending
