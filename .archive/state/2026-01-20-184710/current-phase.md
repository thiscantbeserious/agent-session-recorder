# Current State

## Session Mode: COORDINATOR

This session is a **coordinator** that manages sub-agents. It does NOT implement code directly.

**Key State Files:**
- `.state/plan.md` - Full project plan (version controlled)
- `.state/coordinator.md` - Active agent tracking
- `.state/decisions.md` - Technical decisions log

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
asr skills list                # Show installed skills
asr skills install             # Install skills to agent dirs
asr skills uninstall           # Remove skills from agent dirs
```

### Tests: 102 Total
- 87 unit/integration tests (cargo test)
- 15 e2e tests (tests/e2e_test.sh)

### Key Files
- `AGENTS.md` - Workflow instructions (symlinked as CLAUDE.md, GEMINI.md)
- `tests/e2e_test.sh` - MUST pass before any PR
- `.state/decisions.md` - All technical decisions

## Phase 2: Storage Management - COMPLETE ✅
- [x] Improve `asr status` with more details (PR #4)
- [x] Enhance `asr cleanup` UX (PR #5)
- [x] Add storage threshold warnings after recording (already in Phase 1)
- [x] Improve `asr list` output (PR #6)

## Phase 3: Marker Support - COMPLETE ✅
- [x] Marker functionality (implemented in Phase 1)
- [x] `/asr-analyze` skill documentation
- [x] Skill files (agents/asr-analyze.md, asr-review.md)

## Phase 4: Polish & Distribution - COMPLETE ✅
- [x] Cross-compilation setup (PR #7)
- [x] Homebrew formula (PR #7)
- [x] Skill management CLI - skills embedded in binary (PR #8)
  - `asr skills list/install/uninstall`

## Phase 5: Shell Integration & Automation - NEXT
See `.state/plan.md` for full task list:
- [ ] Global auto-wrap toggle
- [ ] Shell management CLI (asr shell status/install/uninstall)
- [ ] Auto-analyze hook
- [ ] Marked sections in RC files

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
