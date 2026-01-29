# ADR: Play Command

## Status
Accepted

## Context

Users cannot directly play a specific recording file without navigating through the `ls` interface. The native player has powerful capabilities (speed control, seeking, marker navigation) but these are undocumented and only accessible through the TUI.

The requirements call for:
1. A simple `agr play <filepath>` command
2. Documentation of player capabilities in README
3. Reusing the existing native player as-is (no new features)

### Technical Context

The codebase already has:
- `player::play_session(path)` - main entry point in `src/player/native.rs`
- `resolve_file_path()` - path resolution supporting absolute, short format (`agent/file.cast`), and fuzzy matching
- Established command patterns in `src/commands/*.rs`

All existing file-based commands (`analyze`, `marker`, `optimize`) use `resolve_file_path()` for consistency.

## Options Considered

### Option 1: Minimal Command (Recommended)

Add `play` as a simple command that takes a filepath and directly invokes the native player.

- **Pros:**
  - Simplest implementation (~50 LOC)
  - Follows existing command patterns exactly
  - Minimal surface area for bugs
  - Fast to implement and test

- **Cons:**
  - No room for future expansion without refactoring (acceptable per requirements)

### Option 2: Command with Speed Option

Add a `--speed` flag to let users start playback at a different speed.

- **Pros:**
  - More useful for reviewing long recordings quickly

- **Cons:**
  - Out of scope ("reuses existing native player as-is")
  - Would require modifying native player or using asciinema fallback
  - Scope creep

### Option 3: Command with Player Choice

Add `--native` / `--asciinema` flags to choose which player.

- **Pros:**
  - Flexibility for users with preferences

- **Cons:**
  - Exposes implementation detail (two players exist)
  - Confusing UX - why two players?
  - Out of scope

## Decision

**Option 1: Minimal Command**

This option:
1. Matches requirements exactly - "reuses existing native player as-is"
2. Follows established patterns from `analyze`, `marker`, `optimize` commands
3. Keeps the native player's complexity encapsulated
4. Focuses effort on documentation (the higher-value deliverable)

### Path Handling

Use `resolve_file_path()` for consistency with other commands. This supports:
- Absolute paths: `/full/path/to/file.cast`
- Short format: `claude/session.cast` (relative to storage directory)
- Filename only: `session.cast` (fuzzy match across agents)

### Error Messages

Follow the pattern from `analyze` and `optimize`:
```
File not found: <user-provided-path>
Hint: Use format 'agent/file.cast'. Run 'agr list' to see available sessions.
```

## Consequences

### What becomes easier
- Quick playback of known recordings without TUI navigation
- Scripting and automation of playback
- Discoverability of player features via documentation

### What becomes harder
- Nothing significant

### Follow-ups to scope for later
- Speed control option (if user demand emerges)
- Jump-to-marker option
- Integration with `fzf` or similar for fuzzy file selection

## Decision History

1. **Option 1 selected** - User agreed with minimal command approach for simplicity and scope adherence.
2. **Path handling** - Use `resolve_file_path()` for consistency with other commands (supports short format `agent/file.cast`).
3. **Error messaging** - Follow existing pattern with hint to use `agr list`.
