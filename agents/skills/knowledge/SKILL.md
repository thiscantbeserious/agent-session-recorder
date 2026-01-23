---
name: knowledge
description: Project-specific technical knowledge for AGR development. You MUST actually READ the reference files (not just acknowledge them) before doing related tasks. Before using any tool or command, check if a reference file matches by name (e.g., git → git.md) or by similarity (e.g., cargo build → commands.md) and READ it first.
---

# AGR Development Knowledge

**IMPORTANT:** "Load" means use your Read tool to actually read the file contents. Don't just acknowledge the file - READ it and follow its instructions.

READ only what you need for your current task. See the guide below.

## Available References

| File | Load When |
|------|-----------|
| `project.md` | Understanding codebase structure, source files |
| `commands.md` | Running AGR CLI, cargo commands, build scripts |
| `tdd.md` | Writing code with tests, TDD workflow, snapshot testing |
| `verification.md` | Before committing or creating PR |
| `git.md` | Creating branches, PRs, handling CI/CodeRabbit |
| `coding-principles.md` | Writing new code, refactoring, code review |

## Dynamic Loading Guide

### By SDLC Step

| Step | Load These |
|------|------------|
| 1. Requirement | (none - check `.state/` files directly) |
| 2. Design | `project.md`, `coding-principles.md` |
| 3. Code | `tdd.md`, `commands.md`, `coding-principles.md` |
| 4. Test | `verification.md`, `commands.md` |
| 5. Deploy | `git.md`, `verification.md` |
| 6. Feedback | (none - update `.state/` files directly) |

### By Task Type

| Task | Load These |
|------|------------|
| Writing new code | `tdd.md`, `project.md`, `coding-principles.md` |
| Fixing a bug | `tdd.md`, `project.md`, `commands.md` |
| Running tests | `verification.md`, `commands.md` |
| Creating a PR | `git.md`, `verification.md` |
| Understanding codebase | `project.md`, `commands.md` |
| Refactoring code | `coding-principles.md`, `tdd.md` |
| Code review | `coding-principles.md`, `verification.md` |
| TUI/visual changes | `tdd.md` (snapshot testing section) |

### When Unsure

Load all references:
```
references/project.md
references/commands.md
references/tdd.md
references/verification.md
references/git.md
references/coding-principles.md
```
