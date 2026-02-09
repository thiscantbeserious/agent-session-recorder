# PLAN: Improve Filename Template System

**Branch:** `feature/improve-filename-template`
**ADR:** `.state/feature-improve-filename-template/ADR.md`
**Status:** Active

---

## Progress

| Stage | Description | Status |
|-------|-------------|--------|
| 1 | Extract Template Module from `filename.rs` | [x] Done |
| 2 | Introduce `RenderContext` | [x] Done |
| 3 | Add `{branch}` Tag | [x] Done |
| 4 | Add `{id}` Tag (Base36 Epoch) | [x] Done |
| 5 | Add `{?branch}` Optional Syntax | [x] Done |
| 6 | Remove `prompt_rename()` from Recording | [x] Done |
| 7 | Update Default Template and Config Migration | [x] Done |
| 8 | Edge Case Tests | [x] Done |
| 9 | End-to-End Verification | [x] Done |

---

## Execution Stages

Each stage is independently testable. Tests are written before implementation (TDD). Stages are ordered to minimize risk -- foundational refactors first, then new features, then integration.

---

### Stage 1: Extract Template Module from `filename.rs`

**Goal:** Split `filename.rs` into `filename.rs` (sanitization, truncation, rename, `generate()`) and `template.rs` (parsing, rendering, segment types). No behavior changes.

**Scope:**
- Create `src/files/template.rs`
- Move to `template.rs`: `Template`, `Segment`, `TemplateError`, `parse_tag()`, `validate_strftime_format()`, `DEFAULT_DATE_FORMAT`, `DEFAULT_TIME_FORMAT`, `DEFAULT_TEMPLATE`
- Keep in `filename.rs`: `Config`, `sanitize()`, `sanitize_directory()`, `truncate_to_length()`, `generate()`, `validate_length()`, rename functions, `FilenameError`, `GenerateError`
- Update `src/files/mod.rs` to expose `template` module
- Update `filename.rs` to `use super::template::*` where needed
- `generate()` stays in `filename.rs` and delegates to `template::Template`

**Tests (verify before and after):**
- All existing `filename_test.rs` tests pass unchanged
- All existing unit tests in `filename.rs` (if any) pass unchanged
- `cargo clippy` clean

**Verification:** `cargo test --lib --test integration -- filename` passes. No behavior change.

---

### Stage 2: Introduce `RenderContext`

**Goal:** Introduce `RenderContext` struct with `directory` field. Update `render()` and `generate()` signatures. No enum restructuring yet — keep existing `Segment` variants, just change how context is passed.

**Scope:**
- In `template.rs`:
  - Define `RenderContext<'a>` struct: `directory: &'a str`
  - Update `Template::render()` signature to `render(&self, ctx: &RenderContext, config: &Config)`
  - All existing behavior preserved
- In `filename.rs`:
  - Update `generate()` to construct `RenderContext { directory }` and pass to `render()`
- In `recording.rs`:
  - No changes yet (it calls `filename::generate()` which handles the context internally)

**Tests (write first):**
- New unit test: `RenderContext` with directory renders same as before
- All existing `filename_test.rs` tests pass (update imports if `Segment` moved to `template.rs`)

**Verification:** All tests green. `Segment` enum in test file may need import updates.

---

### Stage 3: Add `{branch}` Tag

**Goal:** Implement the `{branch}` tag that resolves to the current git branch name with `/` replaced by `-`.

**Scope:**
- In `template.rs`:
  - Add `Branch` variant to `Segment` enum
  - Add `branch: Option<&'a str>` field to `RenderContext`
  - Add `sanitize_branch()` in `filename.rs`: replaces `/` with `@`, then standard sanitization, no length truncation
  - Update `parse_tag()` to recognize `"branch"` (reject format specifiers: `{branch:...}` = error)
  - Update `render()` to handle `Segment::Branch`:
    - If `ctx.branch` is `Some(b)`: run `sanitize_branch(b)`, emit result
    - If `ctx.branch` is `None`: emit empty string
- In `filename.rs`:
  - Add `pub fn sanitize_branch(input: &str) -> String` — replaces `/` → `@`, then sanitizes
  - Update `generate()` signature to accept `RenderContext` with branch
- In `recording.rs`:
  - Add `fn detect_git_branch() -> Option<String>` utility that runs `git rev-parse --abbrev-ref HEAD`
  - Returns `None` if: git not installed, not a git repo, or detached HEAD (`"HEAD"`)
  - Update `generate_filename()` to call `detect_git_branch()` and include in `RenderContext`

**Tests (write first):**
- Unit: `{branch}` parses without error
- Unit: `{branch}` renders branch name from context
- Unit: `{branch}` replaces `/` with `@` (e.g., `feature/foo` -> `feature@foo`)
- Unit: `sanitize_branch()` handles `@`, `#`, `~` and other git-allowed special chars
- Unit: `sanitize_branch()` does NOT truncate (no length limit)
- Unit: `{branch}` with `None` branch renders empty string
- Unit: `{branch:format}` returns `InvalidFormat` error
- Unit: Template `{directory}_{branch}` renders correctly with branch present and absent
- Integration: `detect_git_branch()` returns `Some` in this repo
- Integration: `detect_git_branch()` returns `None` gracefully when git missing or not a repo

**Verification:** New and existing tests green. `{branch}` is functional.

---

### Stage 4: Add `{id}` Tag (Base36 Epoch)

**Goal:** Implement the `{id}` tag that produces a base36-encoded epoch timestamp.

**Scope:**
- In `template.rs`:
  - Add `Id` variant to `Segment` enum
  - Add `fn encode_base36(value: u64) -> String` utility (inline, ~10 lines, zero-padded to 7 chars)
  - Add `fn epoch_base36() -> String` that calls `encode_base36(now as u64)`
  - Update `parse_tag()` to recognize `"id"` (reject format specifiers)
  - Update `render()` to handle `Segment::Id`: call `epoch_base36()`, emit result

**Tests (write first):**
- Unit: `{id}` parses without error
- Unit: `{id:format}` returns `InvalidFormat` error
- Unit: `encode_base36()` zero-pads to exactly 7 chars
- Unit: `encode_base36(0)` returns `"0000000"`
- Unit: `encode_base36()` with known value (e.g., `1_700_000_000u64` -> expected string)
- Unit: lexicographic sortability: `encode_base36(n)` < `encode_base36(n+1)`
- Unit: Template `{directory}_{id}` renders correctly (directory + underscore + 7 chars)

**Verification:** New and existing tests green.

---

### Stage 5: Add `{?branch}` Optional Syntax

**Goal:** Implement `{?branch}` as a dedicated `OptionalBranch` segment variant. Wraps value in parentheses when present, emits empty when absent. NOT a generic optional system — specific to `{?branch}` only for now.

**Scope:**
- In `template.rs`:
  - Add `OptionalBranch` variant to `Segment` enum
  - Update `parse_tag()` to detect leading `?` in tag content:
    - `?branch` → `Segment::OptionalBranch`
    - `?anything_else` → `TemplateError::UnknownTag` (not supported yet)
  - Update `render()`: for `OptionalBranch`:
    - If `ctx.branch` is `Some(b)`: sanitize, wrap in `(` and `)`, emit
    - If `ctx.branch` is `None`: emit empty string

**Tests (write first):**
- Unit: `{?branch}` parses as `Segment::OptionalBranch`
- Unit: `{?branch}` with branch present renders `(branch-name)`
- Unit: `{?branch}` with branch `feature/foo` renders `(feature@foo)`
- Unit: `{?branch}` with branch absent renders empty string
- Unit: Template `{directory}{?branch}_{id}` with branch renders `dir-name(branch)_0ta73n2`
- Unit: Template `{directory}{?branch}_{id}` without branch renders `dir-name_0ta73n2`
- Unit: No dangling separators when `{?branch}` is empty
- Unit: `{?unknown}` returns `UnknownTag` error
- Unit: `{?}` (empty after ?) returns error
- Unit: `{?date}` returns error (not supported as optional yet)

**Verification:** New and existing tests green.

---

### Stage 6: Remove `prompt_rename()` from Recording

**Goal:** Remove the interactive rename prompt that fires after recording ends. Keep file recovery logic.

**Scope:**
- In `recording.rs`:
  - Remove the interactive stdin prompt from `prompt_rename()` (the `print!("Rename: ")` + `read_line` block)
  - Keep `resolve_actual_path()` — still needed to recover the correct filepath if file was moved during recording
  - Keep `capture_inode()` and `read_header_line()` — still needed by `resolve_actual_path()`
  - Simplify the post-session flow: resolve actual path (for recovery), display `⏹ filename`, continue — no interactive prompt
  - Remove `use atty` import if no longer needed
  - Remove `use std::io::BufRead` if no longer needed (keep `Write` if still used)

**Tests (write first):**
- Verify `Recorder` struct still compiles without `prompt_rename`
- Verify the `record()` method flow: recording ends -> prints filename -> continues to auto-analyze
- No regression in recording behavior (e2e test)

**Verification:** `cargo build` succeeds. Existing e2e tests adapted or confirmed passing.

---

### Stage 7: Update Default Template and Config Migration

**Goal:** Change default template to `{directory}{?branch}_{id}` and migrate existing configs.

**Scope:**
- In `src/config/types.rs`:
  - Change `default_filename_template()` to return `"{directory}{?branch}_{id}"`
- In `src/files/template.rs`:
  - Change `DEFAULT_TEMPLATE` constant to `"{directory}{?branch}_{id}"`
- In `src/config/migrate/mod.rs`:
  - Bump `CURRENT_VERSION` from `1` to `2`
- Create `src/config/migrate/migrations/v2.rs`:
  - `pub const VERSION: u32 = 2;`
  - `pub fn migrate(root: &mut Table, result: &mut MigrateResult)`:
    - Unconditionally set `recording.filename_template` to `{directory}{?branch}_{id}`
- In `src/config/migrate/migrations/mod.rs`:
  - Add `mod v2;`
  - Register `(v2::VERSION, v2::migrate)` in `MIGRATIONS` array
- Update migration snapshot test (`complete_default_config_snapshot`)

**Tests (write first):**
- Unit: v2 migration overwrites old default template
- Unit: v2 migration overwrites custom template (per requirement: unconditional overwrite)
- Unit: v1 config migrates to v2 with new template
- Unit: v2 config is idempotent (re-migration is no-op)
- Unit: `Template::default()` parses the new default template
- Integration: `generate()` with new default template produces expected filename pattern
- Snapshot: update `complete_default_config_snapshot` insta snapshot

**Verification:** All migration tests green. Config round-trip works.

---

### Stage 8: Edge Case Tests

**Goal:** Cover all documented edge cases from the ADR with dedicated tests.

**Scope:**
- EC-1: File recovery after removing prompt_rename
  - Test: `resolve_actual_path()` still works when file was moved (inode recovery)
  - Test: `resolve_actual_path()` returns original path when file exists at expected location
- EC-2: Same-second filename collision
  - Test: `generate()` appends suffix (`a`..`z`) when target file already exists
  - Test: suffix increments correctly (`a`, `b`, `c`...)
  - Test: handles edge case of 26+ collisions gracefully (error or extended suffix)
- EC-3: Git not installed
  - Test: `detect_git_branch()` returns `None` when `git` binary is not found (mock or env)
- EC-4: Detached HEAD
  - Test: `detect_git_branch()` returns `None` when output is `"HEAD"`
- EC-5: Base36 zero-padding sortability
  - Test: `encode_base36(36u64.pow(6) - 1)` (last 6-char unpadded) < `encode_base36(36u64.pow(6))` (first 7-char unpadded) — both are 7 chars when padded
  - Test: `encode_base36(0)` == `"0000000"` (7 zeros)
- EC-6: Branch names with special chars
  - Test: `sanitize_branch("user@feature")` — `@` is kept (it's valid)
  - Test: `sanitize_branch("fix#123")` — `#` handling
  - Test: `sanitize_branch("release~1")` — `~` handling
  - Test: `sanitize_branch("feature/sub/deep")` — multiple `/` → `@`
  - Test: `sanitize_branch("HEAD")` — should not be filtered here (that's `detect_git_branch`'s job)

**Verification:** All edge case tests green.

---

### Stage 9: End-to-End Verification

**Goal:** Verify the full pipeline works: config loads, template parses, branch detected, filename generated, recording completes without prompt.

**Scope:**
- Review/update `tests/e2e_test.sh` if it tests rename prompt behavior
- Run full test suite: `cargo test`
- Run clippy: `cargo clippy`
- Manual smoke test: `agr rec claude` in a git repo, verify filename contains branch name

**Tests:**
- Full `cargo test` passes
- `cargo clippy` clean
- Example output matches requirements:
  - In git repo on `fix/rename-file`: `agent-ses-rec(fix@rename-file)_0ta73n2.cast`
  - Not in git repo: `agent-ses-rec_0ta73n2.cast`

**Verification:** All green. Feature complete.

---

## Stage Dependency Graph

```
Stage 1 (extract template.rs)
  └── Stage 2 (RenderContext + TagKind refactor)
        ├── Stage 3 (add {branch} tag)
        │     └── Stage 5 (add {?tag} syntax) -- depends on {branch} for meaningful optional
        ├── Stage 4 (add {id} tag)
        └── Stage 6 (remove prompt_rename) -- independent of tags
              └── Stage 7 (default template + migration) -- depends on 3, 4, 5, 6
                    └── Stage 8 (edge case tests) -- depends on all features
                          └── Stage 9 (end-to-end verification) -- depends on all
```

Stages 3, 4, and 6 can be done in parallel after Stage 2. Stage 5 depends on Stage 3 (needs `{branch}` to have a meaningful "absent" case). Stage 7 depends on 3, 4, 5, 6 all being complete. Stage 8 is final verification.

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking existing templates | Stage 2 preserves all behavior; all existing tests must pass at each stage |
| Migration overwrites custom templates | Requirement explicitly says unconditional overwrite -- document in migration notes |
| `git` not on PATH | `detect_git_branch()` returns `None` gracefully; `{branch}` renders empty |
| Parentheses in final filename | `sanitize()` strips `()` but only runs on `{directory}` segment, not the final assembled filename |
| File size over 400 lines | Stage 1 splits proactively before adding features |
