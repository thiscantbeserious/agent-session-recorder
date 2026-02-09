# REQUIREMENTS: Improve Filename Template System

**Branch:** `feature/improve-filename-template`
**Status:** Approved
**Sign-off:** Approved by user

---

## Problem Statement

The current filename template system (`{directory}_{date}_{time}`) produces generic timestamps that give no context about what was being worked on. Meanwhile, the interactive rename-on-close prompt (`prompt_rename()` in `src/recording.rs:219`) blocks every recording session, hurting quick-session usability. Since PR #127 shipped an inline rename action in `agr ls`, the post-recording prompt is redundant.

Users need filenames that are self-describing at a glance -- ideally reflecting the git branch context -- without requiring manual intervention after every session.

## Requirements

### REQ-1: Remove rename-on-close prompt

**What:** Remove the interactive rename prompt that fires after every recording session ends.

**Why:** It is a usability blocker for quick sessions. Users who want to rename can do so via `agr ls` (rename action shipped in PR #127).

**Acceptance Criteria:**
- [ ] The `prompt_rename()` method and its call site in `src/recording.rs` are removed
- [ ] After a recording ends, the CLI returns to the shell immediately with no interactive prompt
- [ ] No regression in recording stop/save behavior (file is still written and finalized correctly)

---

### REQ-2: New template tag `{branch}`

**What:** A new template tag that resolves to the current git branch name, sanitized for filenames.

**Why:** Branch names provide immediate context about what work a recording captured, making filenames self-describing.

**Acceptance Criteria:**
- [ ] `{branch}` resolves to the current git branch name (e.g., `main`, `feature@fix-rename`)
- [ ] All `/` characters in branch names are replaced with `@` (e.g., `feature/fix-rename` becomes `feature@fix-rename`)
- [ ] No length limit is applied to branch names (they pass through sanitization but are not truncated)
- [ ] If not inside a git repository, `{branch}` resolves to a sensible fallback (e.g., empty string or `"no-branch"`) -- define behavior clearly in implementation
- [ ] Template parsing accepts `{branch}` as a known tag without error

---

### REQ-3: New template tag `{id}`

**What:** A new template tag that produces a short, unique, sortable identifier derived from the current unix epoch.

**Why:** Replaces verbose `{date}_{time}` for uniqueness with a compact identifier that is still chronologically sortable and unique per second.

**Acceptance Criteria:**
- [ ] `{id}` resolves to the current unix epoch seconds encoded in base36 (lowercase alphanumeric), zero-padded to 7 characters
- [ ] Output is exactly 7 characters (e.g., `0ta73n2`)
- [ ] Values are lexicographically sortable (later timestamps produce later strings)
- [ ] Values are unique per second (same guarantee as `{time}` at second granularity)
- [ ] Template parsing accepts `{id}` as a known tag without error
- [ ] `{id}` does not accept a format specifier (error if `{id:...}` is used)

---

### REQ-4: Optional tag syntax `{?tag}`

**What:** A `?` prefix inside any tag brace (e.g., `{?branch}`) marks it as optional. Optional tags wrap their resolved value in parentheses when present, and render as an empty string when absent.

**Why:** Enables a single default template to work cleanly in both git and non-git contexts without producing ugly artifacts like empty parentheses or dangling separators.

**Acceptance Criteria:**
- [ ] `{?branch}` in a git repo renders as `(branch-name)` (value wrapped in parentheses)
- [ ] `{?branch}` outside a git repo renders as empty string (no output at all -- no empty parens, no placeholder)
- [ ] The `?` prefix works with any tag that can have an absent/empty value (at minimum `{?branch}`)
- [ ] Template parsing recognizes and validates `{?tag}` syntax
- [ ] Optional tags that always have a value (e.g., `{?directory}`, `{?id}`) still wrap in parentheses -- the `?` means "wrap if present, omit if absent"
- [ ] No dangling separators are left when an optional tag renders empty (the template system does not add/remove separators -- the user controls separator placement in the template literal)

---

### REQ-5: Change default template

**What:** Change the default filename template from `{directory}_{date}_{time}` to `{directory}{?branch}_{id}`.

**Why:** The new default produces shorter, more informative filenames. The branch gives context, and `{id}` provides uniqueness more compactly than date+time.

**Acceptance Criteria:**
- [ ] Default template constant is `{directory}{?branch}_{id}`
- [ ] Example output in a git repo on branch `fix/rename-file`: `agent-ses-rec(fix@rename-file)_0ta73n2.cast`
- [ ] Example output outside a git repo: `agent-ses-rec_0ta73n2.cast`
- [ ] No separator between `{directory}` and `{?branch}` (the parentheses from the optional tag serve as visual delimiter)

---

### REQ-6: Preserve existing tags

**What:** The existing `{date}` and `{time}` tags remain fully functional and available for custom templates.

**Why:** Users who prefer timestamp-based filenames can still configure them. Backward compatibility for anyone with a custom `filename_template` in their config.

**Acceptance Criteria:**
- [ ] `{date}` and `{time}` tags continue to work with their current default formats
- [ ] `{date:FORMAT}` and `{time:FORMAT}` custom format specifiers continue to work
- [ ] `{directory}` tag continues to work unchanged
- [ ] Users with existing custom templates in config see no change in behavior

---

### REQ-7: Config migration to enforce new default template

**What:** Bump config version and migrate users on the old default template to the new one.

**Why:** The user wants all installations to adopt the new template, not just new configs. Users with custom templates should be left alone.

**Acceptance Criteria:**
- [ ] Config version is bumped (increment `CURRENT_VERSION`)
- [ ] Migration unconditionally overwrites `filename_template` to `{directory}{?branch}_{id}` regardless of previous value
- [ ] Migration runs automatically on config load (existing migration system)

---

## Out of Scope

- Truncation/length limits on branch names (explicitly not wanted)
- Any changes to the `agr ls` rename functionality
- Template tags beyond `{branch}` and `{id}` (e.g., no `{hostname}`, `{user}`, etc.)

## Dependencies

- Git detection: needs a way to detect current branch (e.g., `git rev-parse --abbrev-ref HEAD` or libgit2)
- Base36 encoding: small utility, can be done inline or with a crate

## Open Questions

None -- requirements are clear from user input.
