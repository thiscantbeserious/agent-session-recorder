# ADR: Improve Filename Template System

**Branch:** `feature/improve-filename-template`
**Status:** Accepted
**Date:** 2026-02-09

---

## Context

The filename template system in `src/files/filename.rs` already supports `{directory}`, `{date}`, `{time}` tags with format specifiers and a parse/render pipeline. The recording module (`src/recording.rs`) calls `prompt_rename()` after every session, which is now redundant after PR #127 shipped inline rename in `agr ls`. Requirements call for new `{branch}`, `{id}` tags, optional `{?tag}` syntax, removing the rename prompt, a new default template, and a config migration.

### Key Observations from Codebase Analysis

1. **File size constraint**: `filename.rs` is already 794 lines (project limit ~400). Any additions must be offset by not adding to this file, or the file needs splitting.
2. **Sanitize removes parentheses**: `sanitize()` strips `(` and `)` characters (line 94-96). The `{?branch}` optional syntax renders as `(value)`. This is safe because `render()` only runs `sanitize_directory()` on the `{directory}` segment -- other segments and literals are emitted raw. The final filename is only validated for length, not re-sanitized.
3. **Migration pattern is well-established**: `v1.rs` shows the pattern -- a `VERSION` constant, a `migrate()` function, registered in `migrations/mod.rs`.
4. **`generate()` signature**: Currently `fn generate(directory: &str, template: &str, config: &Config)`. The new `{branch}` tag needs git context. This signature must change or the render pipeline must accept additional context.
5. **`render()` signature**: Currently `fn render(&self, directory: &str, config: &Config)`. Same issue -- needs branch context.

---

## Design Decisions

### Decision 1: How to Pass Branch Context Through the Template System

The current `render()` accepts only `directory` and `config`. The `{branch}` tag needs the git branch name (or `None` if not in a git repo). This is the central architectural question.

#### Option A: Add a `RenderContext` Struct (Recommended)

Introduce a `RenderContext` struct that bundles all dynamic values needed at render time:

```rust
pub struct RenderContext<'a> {
    pub directory: &'a str,
    pub branch: Option<&'a str>,
}
```

Change `render()` to `render(&self, ctx: &RenderContext, config: &Config)`.

**Pros:**
- Clean extension point -- future tags (hostname, user, etc.) add a field, not a parameter
- Single responsibility: context gathering is separate from rendering
- The caller (`Recorder::generate_filename`) gathers the context, the template system just consumes it
- `generate()` signature becomes `generate(ctx: &RenderContext, template: &str, config: &Config)`

**Cons:**
- Breaking change to `render()` and `generate()` signatures (but both have very few call sites -- 1 each in `recording.rs`)
- Slightly more ceremony for simple use cases

#### Option B: Add `branch: Option<&str>` Parameter Directly

Change `render()` to `render(&self, directory: &str, branch: Option<&str>, config: &Config)`.

**Pros:**
- Minimal change, no new type
- Obvious what each parameter is

**Cons:**
- Each new tag type requires another parameter -- leads to parameter explosion
- Harder to extend later
- `generate()` would need the same parameter added

#### Option C: Make Branch Detection Internal to `render()`

Have `render()` call `git rev-parse --abbrev-ref HEAD` itself when it encounters a `{branch}` segment.

**Pros:**
- No signature change
- Self-contained

**Cons:**
- Violates single responsibility -- template rendering should not shell out to git
- Untestable without a real git repo (or mocking infrastructure the project does not have)
- Performance: runs git on every render, even when template has no `{branch}` tag
- Cannot easily unit-test with controlled branch values

**Recommendation: Option A** -- `RenderContext` struct. It is the cleanest extension point, aligns with the project's single-responsibility principle, and makes testing trivial (pass any branch string without needing git).

---

### Decision 2: How to Implement Optional Tag Syntax `{?tag}`

The `?` prefix changes rendering behavior: wrap in `()` when value is present, emit empty string when absent.

#### Option A: Parse-Time Flag on Segments (Recommended)

Add an `optional: bool` field to each `Segment` variant (or wrap in an `OptionalSegment` wrapper). During parsing, `{?branch}` sets `optional = true`. During rendering, check the flag and wrap/omit accordingly.

Concretely, change `Segment` to:

```rust
pub enum Segment {
    Literal(String),
    Directory { optional: bool },
    Date(String, bool),   // (format, optional)
    Time(String, bool),   // (format, optional)
    Branch { optional: bool },
    Id { optional: bool },
}
```

Or cleaner -- wrap in an `Optional` variant:

```rust
pub enum Segment {
    Literal(String),
    Tag { kind: TagKind, optional: bool },
}

pub enum TagKind {
    Directory,
    Date(String),
    Time(String),
    Branch,
    Id,
}
```

**Pros:**
- Clear separation between tag identity and optional behavior
- Parsing logic stays in `parse_tag()` with a simple `?` prefix check
- Rendering logic has one place to handle the optional wrapping
- `TagKind` + `optional` is more extensible than per-variant booleans

**Cons:**
- Changes the `Segment` enum (breaking for any external consumers -- but this crate has no external API users)

#### Option B: Separate `OptionalTag` Segment Variant

Add `Optional(Box<Segment>)` to the enum. `{?branch}` parses as `Optional(Branch)`.

**Pros:**
- Does not modify existing variants
- Clear nesting

**Cons:**
- Adds pattern-match depth everywhere (render has to unwrap `Optional(inner)` then match `inner`)
- More complex rendering logic

#### Option C: Pre-processing in the Parser

Detect `{?tag}` during parse and emit `Literal("(")`, `Tag`, `Literal(")")` -- but only during rendering, somehow. This does not really work because the parser does not know whether the value will be absent at render time.

**Not viable.**

**User Decision: Implement `{?branch}` as a dedicated `OptionalBranch` segment variant for now.** Do not genericize the optional system yet — keep it specific to `{?branch}`. The generic `TagKind` + `optional` restructuring can come later if more optional tags are needed.

---

### Decision 3: Git Branch Detection

How should the caller (`Recorder`) detect the current git branch?

#### Option A: Shell Out to `git rev-parse` (Recommended)

Run `git rev-parse --abbrev-ref HEAD` via `std::process::Command`.

**Pros:**
- Zero new dependencies
- Proven approach (used by many tools)
- Works in all git repo scenarios (bare, worktrees, detached HEAD)
- Small utility function, easy to test with integration tests

**Cons:**
- Requires `git` binary on PATH
- ~5-10ms per invocation (acceptable -- runs once per recording start)

#### Option B: Use `git2` (libgit2) Crate

Add `git2` as a dependency to read the repo programmatically.

**Pros:**
- No external binary dependency
- Richer git API

**Cons:**
- `git2` is a heavy dependency (~2MB compiled, C bindings, build complexity)
- Overkill for reading one value
- Project already uses `Command` for asciinema; no precedent for libgit2

#### Option C: Read `.git/HEAD` Directly

Parse `.git/HEAD` file to extract the branch ref.

**Pros:**
- No dependency, no process spawn
- Fast

**Cons:**
- Fragile: does not handle worktrees, `gitdir:` indirection, bare repos, or detached HEAD properly
- Reimplements git internals poorly

**Recommendation: Option A** -- shell out to `git rev-parse`. Zero new dependencies, proven, handles edge cases correctly, and is consistent with the project's existing approach of shelling out to external tools.

---

### Decision 4: Base36 ID Implementation

#### Option A: Inline Implementation (Recommended)

Implement base36 encoding as a small function (~10 lines). Base36 of epoch seconds is a simple loop dividing by 36 and mapping to `0-9a-z`. **Zero-pad to 7 characters** to maintain lexicographic sortability across the 6→7 char rollover (which occurs Dec 2038 without padding). With 7-char padding, sortability is guaranteed until year ~4452.

**Pros:**
- Zero dependencies
- Trivial to implement and test
- 7 characters for current epoch (e.g., `0ta73n2`)
- Lexicographically sortable forever (no length-change rollover)

**Cons:**
- Custom code (but very small)
- 7 chars instead of 6 (acceptable tradeoff for correct sorting)

#### Option B: Use a Crate

Add a `base36` or `radix_fmt` crate.

**Pros:**
- Battle-tested

**Cons:**
- Adding a dependency for 10 lines of code
- Supply chain risk for trivial functionality

**Recommendation: Option A** -- inline, zero-padded to 7 chars.

---

### Decision 5: Branch Name Sanitization

Branch names can contain characters beyond `/` that are problematic in filenames: `@`, `#`, `~`, `{`, `}`, and more. The existing `sanitize()` strips `INVALID_CHARS` but does not replace `/` (it removes it, turning `feature/fix` into `featurefix`).

**Decision:** Add a dedicated `sanitize_branch()` function that:
1. Replaces `/` → `@` (preserving namespace visibility, e.g. `feature/fix` → `feature@fix`)
2. Passes through the standard sanitization pipeline for remaining invalid/control chars
3. Does **not** apply any length truncation

This sits alongside `sanitize()` and `sanitize_directory()` in the sanitization module.

---

### Decision 6: Where to Put New Code (File Size)

`filename.rs` is 794 lines, well over the ~400 line guideline. Adding `{branch}`, `{id}`, `{?tag}`, `RenderContext`, `TagKind`, and base36 would push it further.

#### Option A: Split Template Logic into `src/files/template.rs` (Recommended)

Extract the template system (`Template`, `Segment`/`TagKind`, `parse_tag`, `render`, `RenderContext`, base36) into a new `template.rs` module under `src/files/`. Keep `filename.rs` focused on sanitization, truncation, rename, and the top-level `generate()` function.

**Pros:**
- Both files stay under 400 lines
- Clear separation: `template.rs` = parsing and rendering, `filename.rs` = sanitization and filesystem concerns
- `generate()` stays in `filename.rs` as the public entry point, internally using template

**Cons:**
- Requires moving code and updating imports

#### Option B: Keep Everything in `filename.rs`

Just add the new code.

**Pros:**
- Simpler change

**Cons:**
- File would be ~900+ lines, violating the 400-line guideline

**Recommendation: Option A** -- split into `template.rs`. The template system is a self-contained concern that fits cleanly in its own module.

---

## Summary of Recommendations

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Branch context passing | `RenderContext` struct | Extensible, testable, clean |
| Optional syntax | Dedicated `OptionalBranch` variant | Keep it specific for now, genericize later |
| Git branch detection | `git rev-parse` shell-out | Zero deps, proven, graceful fallback |
| Base36 encoding | Inline, zero-padded to 7 chars | Correct sorting, no crate needed |
| Branch sanitization | Dedicated `sanitize_branch()` | `/` → `@`, full special char handling, no truncation |
| File organization | Split into `template.rs` | Keep files under 400 lines |

---

## Edge Cases

### EC-1: File recovery after removing `prompt_rename`

`prompt_rename()` calls `resolve_actual_path()` which handles file-moved-during-recording recovery (inode scan + header fingerprint). Removing the prompt must **not** orphan this logic. The post-recording flow must still call `resolve_actual_path()` to resolve the final filepath — just without the interactive stdin prompt.

### EC-2: Same-second filename collision

Two recordings starting in the same second with identical directory + branch produce the same `{id}`. Since agent recordings are inherently sequential (one agent per terminal), this is extremely unlikely. However, `generate()` should detect an existing file at the target path and append a single-char suffix (`a`..`z`) on collision rather than overwriting.

### EC-3: Git not installed

If `git` is not on PATH, `Command::new("git").output()` returns `Err`. The branch detection function returns `None` gracefully — no panic, no error message, same behavior as "not in a git repo".

### EC-4: Detached HEAD

`git rev-parse --abbrev-ref HEAD` outputs literal `"HEAD"` when detached. This is filtered out and treated as `None` (no branch).

---

## Changes Summary

### Files Modified
- `src/files/filename.rs` -- Extract template types, update `generate()` signature
- `src/recording.rs` -- Remove interactive prompt from `prompt_rename()`, keep `resolve_actual_path()` recovery logic, update `generate_filename()` to gather `RenderContext`
- `src/config/types.rs` -- Update default template constant
- `src/config/migrate/mod.rs` -- Bump `CURRENT_VERSION` to 2
- `src/config/migrate/migrations/mod.rs` -- Register v2 migration
- `tests/integration/filename_test.rs` -- New tests for `{branch}`, `{id}`, `{?tag}`

### Files Created
- `src/files/template.rs` -- Template parsing, rendering, `RenderContext`, `TagKind`, base36
- `src/config/migrate/migrations/v2.rs` -- Migration: overwrite `filename_template`

### Files Deleted
- None (code is moved/refactored, not deleted)
