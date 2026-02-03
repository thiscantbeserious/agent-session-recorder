# Enhanced Shell Minification - Requirements

**Created:** 2026-01-30
**Branch:** enhance-shell-minification
**Base:** main (includes refactor-shell-integration)

---

## Problem Statement

The current shell integration embeds ~50 lines into the user's RC file (`.zshrc`/`.bashrc`). While functional, this is visually intrusive. The existing `minify.rs` only removes comments and blank lines - no actual compression.

**Current state:** ~48 lines output from `agr completions --shell-init zsh`
**Goal:** Single-digit lines (aspirational), low double-digits acceptable

---

## Requirements

### REQ-1: Aggressive Shell Script Compression

**Priority:** P0 (Critical)
**Type:** Enhancement

**Description:** Implement comprehensive compression to minimize embedded line count. Target single-digit lines where feasible.

**Acceptance Criteria:**
- [ ] Compressed output targets **single-digit lines** (best case)
- [ ] Acceptable fallback: **10-15 lines maximum**
- [ ] Output must source correctly in both bash and zsh
- [ ] All completion functionality preserved (commands, subcommands, file completion)
- [ ] Test coverage for compression logic

**Compression Techniques (Architect determines specifics):**
- Statement joining with semicolons
- Function body collapsing
- Case statement inlining
- Whitespace elimination

---

### REQ-2: Debug Mode for Readable Output

**Priority:** P1 (High)
**Type:** Enhancement

**Description:** Provide a way to see expanded/readable output for debugging.

**Acceptance Criteria:**
- [ ] `agr completions --shell-init zsh --debug` outputs readable/expanded version
- [ ] Debug output includes comments explaining each section
- [ ] Default (no flag) outputs compressed version

---

## Constraints

- Must work on macOS and Linux
- Must not break shell function behavior
- Must handle shell syntax edge cases (quoted strings, parameter expansion)
- Builds on existing `src/shell/minify.rs` infrastructure
- **Compression only** - no changes to completion logic itself

---

## Out of Scope

- Fish shell support
- Changes to completion logic itself (only compression of existing output)
- New shell features

---

## Sign-off

- [x] User: Requirements approved (2026-01-30)
