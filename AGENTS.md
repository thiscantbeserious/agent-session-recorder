# Agent Session Recorder (AGR)
A Rust CLI tool for recording AI agent terminal sessions with asciinema.

## File Structure Note
`CLAUDE.md` is a symlink to `AGENTS.md`. Similarly, `.claude/` is a symlink to `agents/`. When committing changes, use the real paths (`AGENTS.md`, `agents/`) not the symlinks.

## Source of Truth
This file is the single source of truth for all AI agents (Claude, Codex, Gemini).

**IMPORTANT: "Load" means actually READ the file contents using your file reading tool (Read, cat, etc). Do NOT just acknowledge the file exists - you MUST read and process its contents.**

### Project Skills
| Skill | Purpose |
|-------|---------|
| `knowledge` | Project-specific technical knowledge - READ files before doing related tasks |
| `architecture` | Agent roles and orchestration patterns |

## First Step: Take a Role
**You MUST read** `agents/skills/architecture/SKILL.md` first, then read the appropriate role file.
Unless instructed otherwise, take the orchestrator role by reading `agents/skills/architecture/references/orchestrator.md`.

## Documentation

| Doc | Purpose |
|-----|---------|
| `agents/skills/architecture/` | Agent roles and orchestration patterns |
| `agents/skills/knowledge/` | Project-specific development knowledge |
| `.state/INDEX.md` | Current state, where to find things |
| `.state/decisions.md` | Technical decisions log |

## Quick Reference

### Before Starting Work
```bash
cat .state/INDEX.md              # Current state
cat .state/decisions.md          # Prior decisions
gh pr list --state merged -L 10  # Recent completed work
```

### Task Steps (SDLC)

## ⚠️ MANDATORY: Read Knowledge Files Before Actions

**You MUST actually READ the knowledge file BEFORE taking any action in that phase.**
Do NOT skip this step. Do NOT assume you know the rules. Project-specific rules override defaults.

| Phase | Action | MUST READ FIRST |
|-------|--------|-----------------|
| Design | Identify files, edge cases | `project.md` |
| Code | TDD, format, lint | `tdd.md` |
| Test | Run tests | `verification.md` |
| Deploy | Create PR, **merge PR** | `git.md` |
| Feedback | Document learnings | `.state/decisions.md` |

Use the `knowledge` skill to load these files.

**This applies to ALL agents** (orchestrator, impl, verify). Fresh sessions must read the relevant file before acting.

**Why:** Project-specific rules (e.g., "never delete branches") are in these files. Skipping causes mistakes.

### Verification Commands
```bash
cargo fmt && cargo clippy && cargo test && ./tests/e2e_test.sh
```

## Project Context

- Records sessions to `~/recorded_agent_sessions/<agent>/`
- Uses asciicast v3 format with native marker support
- AI agents can analyze recordings via `agr analyze <file>` command

## Key Source Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point (clap) |
| `src/lib.rs` | Library root |
| `src/config.rs` | TOML config loading |
| `src/asciicast.rs` | v3 format parser/writer |
| `src/markers.rs` | Marker injection logic |
| `src/storage.rs` | Storage stats & cleanup |
| `src/recording.rs` | asciinema wrapper, rename flow |

## CLI Commands

```
agr record <agent> [-- args...]      # Start recording session
agr analyze <file> [--agent <name>]  # Analyze recording with AI
agr status                           # Show storage stats
agr cleanup                          # Interactive cleanup
agr list [agent]                     # List sessions
agr marker add <file> <time> <label> # Add marker
agr marker list <file>               # List markers
agr agents list                      # List configured agents
agr agents add <name>                # Add agent to config
agr config show                      # Show current config
agr config edit                      # Open config in editor
```

## Reference

- asciicast v3 spec: https://docs.asciinema.org/manual/asciicast/v3/
- Rust impl: https://github.com/asciinema/asciinema/blob/develop/src/asciicast/v3.rs
