# Requirements: Fix Player Scroll Region Bug

## Sign-off

- [x] Requirements reviewed by Product Owner
- [x] Requirements approved by user
- [ ] Implementation complete
- [ ] Validation passed

## Problem Statement

Our native player renders terminal output differently from asciinema and standard terminal emulators (pyte). Content appears at wrong line positions because our VT emulator ignores scroll region commands. Additionally, the escape sequence handling code lacks organization and there's no visibility into unhandled sequences, making future gaps difficult to identify.

**User Impact:** When users play back recordings of TUI applications (vim, tmux, codex CLI, htop, etc.), the displayed content appears at incorrect vertical positions. This makes recordings appear broken or corrupted, degrading trust in the tool and making playback unusable for debugging or review purposes.

## Scope

This is a **bugfix + refactor** with three components:

1. **Bugfix:** Implement scroll region commands and update existing scroll behavior
2. **Refactor:** Extract and organize escape sequence handlers into a central module
3. **Observability:** Add logging for unhandled sequences to catch future gaps

## Investigation Summary

### Root Cause

The test file contains **2,438 scroll region commands** in the first 10,000 events:
- `CSI r` (DECSTBM - Set Top and Bottom Margins): ~2,438 occurrences
- `CSI S` (Scroll Up): ~106 occurrences
- `CSI T` / `ESC M` (Scroll Down / Reverse Index): ~116 occurrences

Our `TerminalBuffer` in `src/player/terminal.rs` has a `_ => {}` catch-all that silently ignores `'r'`, `'S'`, `'T'` sequences.

### Impact

- Content appears at wrong vertical positions (44 lines off in test case)
- Users may think files are corrupted when they're actually fine
- No visibility into what sequences are being ignored

## Acceptance Criteria

### User-Facing Requirements (CRITICAL)

1. [ ] **Visual parity with standard terminals**: Playing the test fixture shows content at the same line positions as pyte/asciinema - NON-NEGOTIABLE
2. [ ] **No regression for simple recordings**: Recordings without scroll regions continue to play correctly
3. [ ] **TUI app compatibility**: Recordings of vim, tmux, codex CLI render correctly

### Test Infrastructure (HIGH)

4. [ ] **Extract test fixture**: Extract relevant scroll region test sections from the investigation recording into an anonymous, reusable test fixture (no personal paths or identifying info)
5. [ ] **Visual comparison test**: Test fixture can be used to verify visual parity with pyte

### Scroll Region Implementation (HIGH)

6. [ ] Add `scroll_top` and `scroll_bottom` fields to track scroll region
7. [ ] Implement `CSI r` handler (DECSTBM - Set Top and Bottom Margins)
8. [ ] Implement `CSI S` handler (Scroll Up within region)
9. [ ] Implement `CSI T` handler (Scroll Down within region)
10. [ ] Update `ESC M` (Reverse Index) to respect scroll region
11. [ ] Update `line_feed` to respect scroll region
12. [ ] Reset scroll region on terminal resize

### Code Refactor (HIGH)

13. [ ] **Extract handlers into methods**: Each escape sequence handler should be its own method, not inline in the match
14. [ ] **Organize in central module**: Group handlers logically (cursor movement, scrolling, editing, etc.) with clear structure
15. [ ] **Audit for other unhandled sequences**: Check what else is being silently ignored by the `_ => {}` catch-all

### Observability (HIGH)

16. [ ] **Log unhandled sequences**: Add debug/warning tracing for any escape sequences that hit the catch-all, so future gaps are visible

### Edge Cases (MEDIUM)

17. [ ] Invalid scroll region params handled gracefully (ignored or clamped)
18. [ ] Cursor behavior correct when inside/outside scroll region

## Out of Scope

- DECOM (origin mode) - cursor positioning remains absolute
- Full xterm/vt100 edge case compatibility
- Performance optimization
- Alternate screen buffer handling

## Test Fixture Requirements

The existing investigation used a local recording at a personal path. For the implementation:

1. **Extract only the relevant sections** that exercise scroll region commands
2. **Keep it anonymous** - no personal paths or identifying information
3. **Make it reusable** - place in a test fixtures directory as part of the codebase
4. **Document what it tests** - the fixture should clearly indicate it's for scroll region verification

## Definition of Done

- [ ] Visual parity test passes using the extracted fixture
- [ ] All existing terminal tests pass (`cargo test`)
- [ ] Escape sequence handlers are organized in a central module with clear grouping
- [ ] Unhandled sequences produce debug/trace output
- [ ] Code reviewed by Reviewer role
- [ ] Tests pass in CI
- [ ] Product Owner validates user-facing requirements are met

## Technical Notes

### Example scroll region sequence from test file:
```
\x1b[?2026h\x1b[1;69r\x1b[4S\x1b[r
```
Breakdown:
- `\x1b[?2026h` - DEC private mode (ignored, fine)
- `\x1b[1;69r` - Set scroll region lines 1-69
- `\x1b[4S` - Scroll up 4 lines
- `\x1b[r` - Reset scroll region to full screen

### Reference implementations:
- pyte (Python): https://github.com/selectel/pyte
- vte (Rust crate we use): Already parses these, we just don't handle them

## Context

- Branch: `fix/player-scroll-region-bug`
- ADR approved with implementation design
- pyte installed for reference comparison: `pip3 install pyte`
