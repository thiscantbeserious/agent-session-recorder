# Agent Session Recorder (AGR)
A Rust CLI tool for recording AI agent terminal sessions with asciinema.

## Source of Truth
This file is the single source of truth for all AI agents (Claude, Codex, Gemini).

### Project Skills
| Skill | Purpose |
|-------|---------|
| `knowledge` | Project-specific development knowledge (git, tdd, rust, state) |
| `architecture` | Agent roles and orchestration patterns |

## Always take a role
Read the architecture skill, unless your instructed to take a specific role take the orchestrator role

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
1. **Requirement** - Check state and decisions
2. **Design** - Identify files, consider edge cases
3. **Code** - TDD (Red-Green-Refactor), format, lint
4. **Test** - `cargo test` and `./tests/e2e_test.sh`
5. **Deploy** - Create PR, wait for CI + CodeRabbit
6. **Feedback** - Document blockers and learnings

For detailed instructions, load `agents/skills/knowledge/references/tdd.md`.

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
