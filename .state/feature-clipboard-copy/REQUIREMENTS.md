# Requirements: Clipboard Copy Feature

**Branch:** feature-clipboard-copy
**Created:** 2026-02-02
**Sign-off:** Approved by user

---

## Problem Statement

Users want to share `.cast` recording files on Slack and similar platforms. Currently, there's no way to copy a recording file to the clipboard from within the tool. Users must manually navigate to the recordings directory and drag/drop or use OS-level file operations.

## Desired Outcome

Users can copy a `.cast` recording file to their system clipboard with a single action, then paste it directly into Slack (or other applications) as a file attachment.

## Scope

### In Scope

1. **CLI command** - New `agr copy <recording>` command to copy a recording file to clipboard
2. **TUI integration** - Copy option in the file explorer:
   - Added to the action dialog (appears when pressing Enter on a recording in `ls`)
   - Keybinding shortcut for quick access
   - Listed in help text
3. **Cross-platform support** - macOS and Linux only
4. **Feedback** - Display detailed confirmation: "Copied recording-2024-01-15.cast to clipboard"
5. **Graceful fallback** - On platforms where file-to-clipboard isn't supported, copy the `.cast` file content (JSON text) to clipboard instead, with appropriate messaging
6. **Full documentation** - README updates, help text, and any other relevant documentation

### Out of Scope

- Windows support (explicitly excluded)
- Copying non-`.cast` files (this tool is specifically for recordings)
- Copying file paths or links (user wants actual file/content)
- Integration with specific apps (Slack, etc.) - just clipboard
- Batch copy of multiple recordings

## Acceptance Criteria

### CLI Command (`agr copy`)

1. `agr copy <recording-name>` copies the specified `.cast` file to clipboard
2. Recording can be specified by name (with or without `.cast` extension)
3. On success, displays: "Copied <filename>.cast to clipboard"
4. On fallback (content copy), displays: "Copied <filename>.cast content to clipboard (file copy not supported on this platform)"
5. On error (file not found, etc.), displays appropriate error message

### TUI File Explorer

1. **Action dialog** - "Copy to clipboard" option in the dialog that appears when pressing Enter on a recording
2. **Keybinding** - Direct keyboard shortcut to copy selected recording
3. **Help text** - Copy action documented in TUI help
4. Feedback message appears in the TUI after copy
5. Same fallback behavior as CLI

### Cross-Platform Behavior

| Platform | Primary Method | Fallback |
|----------|----------------|----------|
| macOS | File reference to clipboard (osascript) | Copy file content as text |
| Linux | File reference to clipboard (xclip/xsel/wl-copy) | Copy file content as text |

### Error Cases

1. Recording not found - clear error message
2. Clipboard access fails - clear error message
3. No clipboard tool available - fall back to content copy with explanation

### Documentation

1. README updated with copy feature usage
2. CLI help text documents `agr copy` command
3. TUI help text documents copy keybinding

## Constraints

- **TDD methodology** - Implementation must follow Test-Driven Development (Red/Green):
  1. Write failing tests first
  2. Implement code to make tests pass
  3. Refactor as needed
- Must not introduce heavy dependencies if avoidable
- Should detect available clipboard tools at runtime rather than compile-time platform checks where possible

## Open Questions for Architect

1. Which keybinding to use in the TUI explorer (suggest `y` for "yank" or `c` for "copy")
2. Whether to use a Rust clipboard crate or shell out to OS tools
3. Specific clipboard tools to support on Linux (xclip, xsel, wl-copy for Wayland)

---

## Sign-off

- [x] **Approved by user** (2026-02-02)
