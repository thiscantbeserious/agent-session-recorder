# Current State

## Session Mode: COORDINATOR

This session is a **coordinator** that manages sub-agents. It does NOT implement code directly.

See `.state/coordinator.md` for active agent tracking.

## Quick Start for New Session
```bash
cd ~/git/simon/agent-session-recorder
export PATH="$HOME/.cargo/bin:$PATH"    # Load Rust
cargo test && ./tests/e2e_test.sh       # Verify everything works
```

## Project Info
- **Location:** ~/git/simon/agent-session-recorder/
- **Repo:** github.com/thiscantbeserious/agent-session-record
- **License:** MIT

## Phase 1: COMPLETE (Merged)
PR #1 merged to main. All core functionality working.

### What's Implemented
| Module | File | Purpose |
|--------|------|---------|
| Config | src/config.rs | TOML config, ~/.config/asr/config.toml |
| Asciicast | src/asciicast.rs | v3 parser/writer, markers, exit events |
| Markers | src/markers.rs | Add/list markers in .cast files |
| Storage | src/storage.rs | Session listing, stats, cleanup |
| Recording | src/recording.rs | asciinema wrapper, rename flow |
| CLI | src/main.rs | All commands via clap |

### CLI Commands (all working)
```
asr record <agent> [-- args]   # Record with asciinema
asr status                     # Storage stats
asr cleanup                    # Interactive cleanup
asr list [agent]               # List sessions
asr marker add <f> <t> <label> # Add marker at timestamp
asr marker list <file>         # List markers
asr agents list                # Show configured agents
asr agents add/remove <name>   # Modify agent list
asr config show                # Display config
asr config edit                # Open in $EDITOR
```

### Tests: 79 Total
- 41 unit tests (in src/*.rs)
- 23 integration tests (tests/*.rs)
- 15 e2e tests (tests/e2e_test.sh)

### Key Files
- `AGENTS.md` - Workflow instructions (symlinked as CLAUDE.md, GEMINI.md)
- `tests/e2e_test.sh` - MUST pass before any PR
- `.state/decisions.md` - All technical decisions

## Phase 2: Storage Management (NOT STARTED)
- [ ] Improve `asr status` with more details
- [ ] Enhance `asr cleanup` UX
- [ ] Add storage threshold warnings after recording
- [ ] Improve `asr list` output

## Phase 3: Marker Support (NOT STARTED)
- [ ] Enhance marker functionality
- [ ] Test `/asr-analyze` skill end-to-end

## Phase 4: Polish (NOT STARTED)
- [ ] Cross-compilation for Linux
- [ ] Finalize Homebrew formula
- [ ] CI/CD setup

## Git Workflow
```bash
# Always create feature branch from main
git checkout main && git pull
git checkout -b feature/phase2-task-name

# Work, commit frequently
cargo test && ./tests/e2e_test.sh  # MUST pass
git add -A && git commit -m "feat(scope): description"

# Push and create PR
git push -u origin feature/phase2-task-name
gh pr create --base main --title "feat: ..."

# After approval, merge WITHOUT deleting branch
gh pr merge N --squash  # NO --delete-branch
```

## Important Notes
1. **Cargo env:** Run `. "$HOME/.cargo/env"` in new shells
2. **E2E tests:** MANDATORY before any merge
3. **Branches:** NEVER delete - keep all feature branches
4. **Config path:** ~/.config/asr/ on ALL platforms (not ~/Library/...)
