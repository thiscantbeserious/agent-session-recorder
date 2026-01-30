# ADR: Aggressive Shell Script Compression

**Date:** 2026-01-30
**Status:** Accepted
**Branch:** enhance-shell-minification

---

## Context

The shell integration currently embeds ~48 lines into user RC files. While functional, this is visually intrusive. The existing `minify.rs` only removes comments and blank lines - no actual compression occurs.

User wants minimal footprint in their shell configuration files.

---

## Research: shfmt Analysis

Analyzed shfmt (Go-based shell formatter) with `-mn` flags for minification insights.

### Key Findings

1. **Parser-based approach** - Uses full AST, not regex
2. **Does NOT support zsh** - Cannot parse `${(f)...}` parameter expansion syntax
3. **Does NOT join top-level statements** - Preserves newlines between commands
4. **Achieved 39% line reduction** on bash test (46 to 28 lines)

### Techniques shfmt Uses

- Strip all comments except shebang
- Remove indentation
- Remove spaces around operators (`&&`, `||`, `|`)
- Join keyword/condition: `if cond;then`, `for x in y;do`
- Preserve heredoc content verbatim
- Handle `< <(...)` space requirement
- Remove final `;;` before `esac`

### Where We Can Exceed shfmt

- Join top-level statements with `;` (shfmt intentionally doesn't)
- Support zsh-specific syntax (`${(f)...}`, `setopt`, etc.)
- Target true single-line output per function

---

## Decision

**Selected: Option B - Aggressive Single-Line Compression**

**Targets (aligned with REQUIREMENTS):**
- **Aspirational:** Single-digit lines (6-8)
- **Acceptable:** 10-15 lines maximum

### Rationale

- Maximum visual cleanliness in RC files
- Shell scripts are write-once, read-rarely for completions
- Debug flag provides escape hatch for troubleshooting
- User explicitly chose aggressive approach for risk/reward tradeoff

---

## Rejected Alternatives

### Why Not Use Parser Crates?

Several Rust shell parser crates were evaluated and rejected:

1. **conch-parser**
   - Archived since May 2022
   - Uses Rust 2015 edition (outdated)
   - No zsh support whatsoever
   - Unmaintained - security and compatibility concerns

2. **yash-syntax**
   - Actively maintained
   - POSIX-only implementation
   - Does not support zsh parameter expansion flags (`${(f)...}`)
   - Would require significant patches for our use case

3. **tree-sitter-zsh**
   - Repository archived
   - Minimal development history
   - Grammar incomplete for production use

4. **tree-sitter-bash**
   - Good for bash, but no zsh support
   - Tree-sitter adds significant dependency overhead

### Conclusion

A line-by-line processing approach is the pragmatic choice because:
- We control the generated input (our own templates)
- We require zsh syntax support (not available in existing parsers)
- The input is predictable and well-structured
- Full AST parsing is overkill for known, controlled input
- Simpler implementation, easier to debug and maintain

---

## Implementation Methodology

### TDD Approach: Red/Green/Refactor

This feature will be implemented using strict Test-Driven Development:

1. **RED Phase** - Write comprehensive failing tests first
   - All tests defined before any implementation
   - Tests live in `tests/integration/shell/minify_test.rs`
   - Each test category covers a specific compression technique
   - Tests must fail initially (no implementation exists)

2. **GREEN Phase** - Implement minimum code to pass tests
   - Implement one category at a time
   - Run tests after each implementation step
   - Focus on correctness, not optimization

3. **REFACTOR Phase** - Clean up implementation
   - Improve code quality after tests pass
   - Ensure no regression (tests still pass)
   - Add documentation and clean up structure

### Test Coverage Requirements

- Unit tests for each compression technique
- Integration tests for full minification pipeline
- **Functional tests** verifying completions work after minification
- Snapshot tests for regression detection
- Debug mode tests (comments present in output)

---

## Implementation Approach

Extending existing `src/shell/minify.rs` with aggressive compression (not creating new module).

### Integration Point

Minification is called in `src/shell/completions.rs` within `generate_zsh_init()` and `generate_bash_init()`. The `--debug` flag will be passed through the CLI to control compression level.

```
CLI (--debug flag)
    └─> completions.rs (generate_*_init)
            └─> minify.rs (minify or minify_aggressive)
                    └─> Output (compressed or readable)
```

### Compression Techniques

1. **Statement Joining** - Join sequential statements with `;`
   - `echo a` + `echo b` -> `echo a;echo b`

2. **Function Body Collapsing** - Single-line function bodies
   - `_fn() {\n  cmd\n}` -> `_fn() { cmd; }`

3. **Case/If Inlining** - Collapse control structures
   - `if cond;then`, `for x in y;do` (from shfmt)
   - Case patterns on single lines where feasible

4. **Whitespace Elimination** - Remove non-essential spaces
   - Remove spaces around `&&`, `||`, `|`
   - Preserve spaces in strings and required syntax

### Safety Rules (from shfmt analysis)

- **Process substitution**: `< <(cmd)` must preserve space (otherwise `<<` is heredoc)
- **Heredoc content**: Never modified, preserved verbatim
- **Word boundaries**: `echo$var` invalid, need space or quote separation
- **Quoted strings**: Preserve content and expansions intact
- Never join across control structure boundaries (`do`, `done`, `fi`, `esac`)
- Preserve newlines after `{` and before `}` when deeply nested

### Processing Strategy

1. Track quote state (in single/double quote or not)
2. Identify safe join points between statements
3. Preserve content inside quotes verbatim
4. Handle heredocs specially (pass through unchanged)
5. Apply operator spacing rules outside quotes

---

## Scope Constraints

**In Scope:**
- Compression of shell script output
- Debug mode flag
- Tests for minification

**Out of Scope (per REQUIREMENTS):**
- Changes to completion logic itself
- Fish shell support
- New shell features

The implementer must NOT modify completion generation logic - only the minification step.

---

## Debug Mode

`--debug` flag outputs expanded, commented version for troubleshooting:
- Full formatting preserved
- Section comments included (explaining each part)
- Default (no flag) outputs compressed version

---

## Consequences

**Positive:**
- Minimal RC file footprint (target 6-8 lines, max 10-15)
- Clean user experience
- Debug mode available when needed
- zsh support (unlike shfmt)

**Negative:**
- Compressed output harder to read inline
- Slightly more complex minification logic
- Edge cases in shell syntax require careful handling

**Mitigations:**
- Debug flag for readable output
- Comprehensive test coverage
- Snapshot tests for regression detection

---

## Platform Testing

Must work on macOS and Linux. CI pipeline runs on both platforms - all tests must pass on both.

---

## Sign-off

- [x] User: ADR approved (2026-01-30)
