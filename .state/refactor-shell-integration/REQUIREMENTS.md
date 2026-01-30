# Shell Integration Refactor - Requirements Document

**Created:** 2026-01-30
**Status:** Draft - Pending Sign-off
**Requested By:** User
**Product Owner:** Agent (Claude)

---

## Executive Summary

Rewrite the shell integration subsystem with a new architecture. Replace the broken completion scripts entirely with a clean, auto-generated system. Add ghost text autosuggestions for file-accepting commands, minify shell scripts when embedding, and refactor the Rust code into modular components.

---

## Problem Statement

### Current Issues

1. **Outdated Command List**: Static completions reference `record, status, cleanup, list, analyze, marker, agents, config, shell` but CLI also has:
   - `play` - Play recordings with native player
   - `optimize` - Remove silence from recordings
   - `ls` - Alias for `list`
   - `completions` (hidden) - Internal completion helper

2. **Broken Completions**: User reports completions "are static and don't work" - likely stale command lists and missing dynamic file suggestions for new commands.

3. **No Ghost Text Preview**: User wants Fish/zsh-autosuggestions style inline suggestions with "greyish letters and slight background" showing the most likely file completion when typing commands like `agr cleanup`, `agr ls`, `agr play`.

4. **No Minification**: Full shell script (128 lines with comments) is embedded verbatim in `.zshrc`/`.bashrc`, wasting visual space.

5. **Monolithic Code Structure**: `src/shell.rs` (338 lines) handles everything - installation, uninstallation, path detection, status, completions installation, etc.

---

## Requirements

### REQ-1: Replace Completion System with New Architecture

**Priority:** P0 (Critical)
**Type:** Rewrite

**Description:** Scrap existing `completions.bash` and `completions.zsh` entirely. Design a new completion architecture from scratch that is maintainable, auto-generates from CLI structure, and supports ghost text natively.

**Rationale:** Current completion scripts are:
- Manually maintained duplicates (150+ lines each)
- Out of sync with CLI commands
- Not designed for ghost text integration
- Hard to extend when adding new commands

**Acceptance Criteria:**
- [ ] Remove `shell/completions.bash` and `shell/completions.zsh`
- [ ] New completion system auto-derives commands from `clap` CLI definition
- [ ] Single source of truth for command structure
- [ ] Built-in support for ghost text suggestions (not bolted on)
- [ ] Clean integration with `agr.sh` shell script
- [ ] Works on both bash and zsh without duplicated logic where possible
- [ ] Dynamic file completion for file-accepting commands

**Design Principles:**
- Generate completions at runtime or install-time from CLI metadata
- Minimal shell code, maximum Rust logic (via `agr completions` subcommand)
- Ghost text as first-class feature, not afterthought

---

### REQ-2: Shell Script Minification

**Priority:** P1 (High)
**Type:** Enhancement

**Description:** Compress the shell script when embedding into RC files to reduce visual clutter while preserving functionality.

**Acceptance Criteria:**
- [ ] Implement minification that:
  - Removes comment lines (lines starting with `#` except shebang)
  - Removes blank/empty lines
  - Preserves string literals containing `#` characters
  - Preserves inline comments only if they affect functionality (none expected)
- [ ] Minified output still sources correctly in bash and zsh
- [ ] Original `shell/agr.sh` remains readable/documented (minification happens at embed time only)
- [ ] Add test coverage for minification logic

**Constraints:**
- Must work on macOS and Linux
- Must not break any shell function behavior
- Consider keeping a "pretty" mode for debugging (`AGR_DEBUG=1`)

---

### REQ-3: Ghost Text Autosuggestions for File Commands

**Priority:** P1 (High)
**Type:** New Feature

**Description:** When user types a file-accepting command (e.g., `agr play `, `agr cleanup `, `agr ls `), show the most likely file completion as ghost text (grayed out, inline) that can be accepted with Tab or right-arrow.

**Behavior:**
1. If no input after command: Show the most recent/relevant file as ghost suggestion
2. If partial input: Filter and show best match
3. Tab accepts the ghost suggestion (standard completion behavior)
4. Ghost text styling: gray/dim foreground, optionally slight background

**File-Accepting Commands:**
- `agr play <file>` - cast file
- `agr analyze <file>` - cast file
- `agr optimize <file>` - cast file
- `agr marker add <file>` - cast file
- `agr marker list <file>` - cast file
- `agr cleanup` (optional file filter, but typically no file arg)
- `agr list` / `agr ls` (optional agent filter, not file)

**Acceptance Criteria:**
- [ ] Zsh implementation using ZLE widgets or zsh-autosuggestions compatible hooks
- [ ] Bash implementation using readline hooks or PROMPT_COMMAND
- [ ] Ghost text appears inline after cursor in dim/gray styling
- [ ] Tab completes the suggestion
- [ ] Empty suggestion when no files match
- [ ] Quick list popup when user explicitly requests (e.g., double-Tab shows menu)
- [ ] Ranked suggestions: most recent files first

**Technical Approach (Suggested):**
- Zsh: Custom ZLE widget binding, or integrate with existing `zsh-autosuggestions` plugin pattern
- Bash: `bind -x` with readline, or PROMPT_COMMAND with cursor positioning

**Constraints:**
- Must not conflict with existing completion systems
- Must work without external dependencies (no mandatory zsh-autosuggestions plugin)
- Should degrade gracefully if terminal doesn't support styling

---

### REQ-4: Refactor Shell Module Structure

**Priority:** P2 (Medium)
**Type:** Code Quality

**Description:** Split monolithic `src/shell.rs` into modular components for maintainability.

**Proposed Structure:**
```
src/shell/
  mod.rs          # Public API re-exports
  install.rs      # RC file installation/uninstallation
  status.rs       # Status detection and reporting
  minify.rs       # Script minification logic
  paths.rs        # Path detection (RC files, completion dirs)
  completions.rs  # Completion file installation
```

**Acceptance Criteria:**
- [ ] Split into at least 4 logical modules
- [ ] No change to public API (`agr::shell::*` exports remain compatible)
- [ ] All existing tests pass
- [ ] New modules have focused responsibilities
- [ ] Add module-level documentation

---

### REQ-5: Update Completions Backend (CLI Side)

**Priority:** P1 (High)
**Type:** Enhancement

**Description:** Enhance `agr completions --files` to support ranked/sorted output for better autosuggestions.

**Acceptance Criteria:**
- [ ] `agr completions --files` returns files sorted by modification time (most recent first)
- [ ] Support `--limit N` flag to cap results (default: 10 for ghost text, unlimited for menu)
- [ ] Consider adding `--format=json` for richer metadata (filename, mtime, agent)

---

## Non-Requirements (Out of Scope)

- Fish shell support (user didn't mention, focus on bash/zsh)
- Windows PowerShell support
- Integration with external autosuggestion plugins (should work standalone)
- Changing the marker format or asciicast file structure
- Any changes to recording/playback functionality

---

## Technical Constraints

1. **Cross-Platform:** Must work on macOS (zsh default) and Linux (bash common)
2. **No New Dependencies:** Shell integration must work with standard bash/zsh
3. **Backward Compatibility:** Existing `agr shell install` users should auto-upgrade cleanly
4. **Performance:** Ghost text lookup should complete in <100ms to feel instant

---

## Testing Requirements

- [ ] Unit tests for minification logic
- [ ] Integration tests for completion scripts (both shells)
- [ ] Manual testing on macOS (zsh) and Linux (bash)
- [ ] Test upgrade path from old embedded script to new minified version
- [ ] Test ghost text in terminals with/without color support

---

## Delivery Phases (Suggested)

| Phase | Requirements | Rationale |
|-------|--------------|-----------|
| 1 | REQ-4 (Refactor Shell Module) | Clean foundation first |
| 2 | REQ-2 (Minification) | Quick win, low risk |
| 3 | REQ-1 (New Completion Architecture), REQ-5 (Backend) | Core rewrite with proper backend |
| 4 | REQ-3 (Ghost Text) | Main UX feature on clean base |

---

## Sign-off

- [ ] **Product Owner:** Requirements complete and accurate
- [ ] **User:** Requirements match expectations

---

## Resolved Questions

1. **Ghost text trigger:** Immediately on space - no delay
2. **Fallback behavior:** Show nothing if no files match (no placeholder)
3. **Double-Tab behavior:** Does nothing after completion is accepted - already complete

---

## Sign-off

- [x] **User:** Requirements match expectations (2026-01-30)

---

*Document generated by Product Owner agent for shell integration refactor initiative.*
