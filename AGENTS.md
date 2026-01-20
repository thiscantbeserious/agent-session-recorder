# Agent Session Recorder (ASR)

A Rust CLI tool for recording AI agent terminal sessions with asciinema.

## IMPORTANT: Agile SDLC Workflow

**Before doing anything, read `.state/current-phase.md`** to understand context.

### For Each Task, Follow These Steps:

#### 1. Requirement Gathering
```bash
cat .state/current-phase.md      # Current context
cat .state/phase-N/progress.md   # Find next task
cat .state/decisions.md          # Prior decisions
```

#### 2. Design
- Identify files to create/modify
- Consider edge cases
- Document design in `.state/decisions.md` if significant

#### 3. Coding (TDD Sprint)
**Follow Red-Green-Refactor:**
1. Write failing test first (behavior-focused)
2. Run test → must fail
3. Write minimal code to pass
4. Run test → must pass
5. Refactor if needed
6. Commit

#### 4. Testing / QA
- All unit tests must pass: `cargo test`
- Coverage should be ≥80%
- **E2E tests must pass: `./tests/e2e_test.sh`** (requires asciinema)
- Run verification command (see table below)
- Log results to `.state/phase-N/test-results.md`

#### 5. Deployment
- Build release: `cargo build --release`
- Run e2e tests: `./tests/e2e_test.sh`
- If all tests pass: mark `[x]` in `progress.md`
- Update `.state/current-phase.md` with next task

#### 6. Feedback
- Document blockers in `.state/phase-N/blockers.md`
- Note lessons learned in `.state/decisions.md`
- If blocked: move to next task

### Task Locking (for parallel subagents)

**Before working on a task:**
```bash
# Check if task is claimed
if [ -f .state/locks/task-name.lock ]; then
  echo "Task claimed, pick another"
  exit 0
fi
# Claim it
echo "$(date +%s)" > .state/locks/task-name.lock
```

**After completing a task:**
```bash
rm .state/locks/task-name.lock
```

## Project Context

- Records sessions to `~/recorded_agent_sessions/<agent>/`
- Uses asciicast v3 format with native marker support
- AI agents analyze recordings and add markers via `/asr-analyze` skill

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
asr record <agent> [-- args...]   # Start recording session
asr status                        # Show storage stats
asr cleanup                       # Interactive cleanup
asr list [agent]                  # List sessions
asr marker add <file> <time> <label>  # Add marker
asr marker list <file>            # List markers
asr agents list                   # List configured agents
asr agents add <name>             # Add agent to config
asr config show                   # Show current config
asr config edit                   # Open config in editor
```

## Verification Commands

**MANDATORY: Run these before every PR/commit:**

```bash
# 1. Unit tests
cargo test

# 2. Build release binary
cargo build --release

# 3. E2E tests with real asciinema
./tests/e2e_test.sh
```

| Task | Test Command |
|------|--------------|
| Unit tests | `cargo test` |
| E2E tests | `./tests/e2e_test.sh` |
| Docker build | `./build.sh && ls dist/` |
| All commands | Run e2e_test.sh (tests all CLI commands with real asciinema) |

## Reference

- asciicast v3 spec: https://docs.asciinema.org/manual/asciicast/v3/
- Rust impl: https://github.com/asciinema/asciinema/blob/develop/src/asciicast/v3.rs

## Building

```bash
./build.sh            # Docker build (runs tests + builds binary)
./install.sh          # Install to system
./uninstall.sh        # Remove from system
```

## Development Phases

### Phase 1: MVP (Core Recording) - IN PROGRESS
- [x] Project setup (Cargo.toml, basic structure)
- [x] Docker build environment
- [ ] `asr record <agent>` - spawn asciinema
- [ ] Rename prompt on normal exit
- [ ] Keep original filename on Ctrl+C
- [ ] `asr agents list/add`
- [ ] Shell wrapper
- [ ] Install script

### Phase 2: Storage Management
- [ ] `asr status`
- [ ] `asr cleanup`
- [ ] Storage threshold warnings
- [ ] `asr list`

### Phase 3: Marker Support
- [ ] `asr marker add`
- [ ] `asr marker list`
- [ ] `/asr-analyze` skill

### Phase 4: Polish
- [ ] Config file support
- [ ] README documentation
- [ ] Cross-compilation
- [ ] Homebrew formula

## Git Workflow

```bash
# Feature branch
git checkout -b feature/phase1-task-name

# After implementation
git add -A
git commit -m "feat(scope): description"
git push -u origin feature/phase1-task-name

# Create PR
gh pr create --title "feat(scope): description"

# After review & merge
git checkout main
git pull
```
