# Agent Session Recorder (ASR)

A Rust CLI tool for recording AI agent terminal sessions with asciinema.

## IMPORTANT: Agile SDLC Workflow

**Before doing anything, read the state files** to understand context.

### Key State Files
- `.state/plan.md` - **Master plan** with phase status and all tasks
- `.state/current-phase.md` - Current session context
- `.state/coordinator.md` - Agent tracking (if coordinator session)
- `.state/decisions.md` - Technical decisions log

### For Each Task, Follow These Steps:

#### 1. Requirement Gathering
```bash
cat .state/plan.md               # Master plan with phase tasks
cat .state/current-phase.md      # Current context
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
6. **Format code:** `cargo fmt`
7. **Lint code:** `cargo clippy`
8. Commit

**Code Quality Checks (MANDATORY before commit):**
```bash
cargo fmt          # Format code
cargo clippy       # Lint for common issues
cargo test         # Run all tests
```

#### 4. Testing / QA
- All unit tests must pass: `cargo test`
- Coverage should be ≥80%
- **E2E tests must pass: `./tests/e2e_test.sh`** (requires asciinema)
- Run verification command (see table below)
- Log results to `.state/phase-N/test-results.md`

#### 5. Pull Request & CI
**MANDATORY: Wait for CI to pass before merging!**

1. Create PR: `gh pr create --title "feat(scope): description"`
2. Wait for all CI checks to pass:
   ```bash
   gh pr checks <PR_NUMBER>   # Check status
   ```
3. CI Pipeline stages:
   - **Build** - compilation on ubuntu + macos
   - **Unit Tests** - cargo test on both OS
   - **E2E Tests** - integration tests (macOS only)
   - **Lint** - cargo fmt + clippy
   - **CodeRabbit** - AI code review
4. **NEVER merge until ALL checks show `pass`**
5. If checks fail, fix issues and push again
6. Merge only after: `gh pr checks` shows all green

#### 6. Deployment
- Build release: `cargo build --release`
- Run e2e tests: `./tests/e2e_test.sh`
- If all tests pass: mark `[x]` in `progress.md`
- Update `.state/current-phase.md` with next task

#### 7. Feedback
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

## AI Agent Skills

These skills are for AI agents (Claude, Codex, Gemini) to use, not CLI commands.

### `/asr-analyze <file.cast>`

Analyze a session recording and add markers for interesting moments.

**How to use:**
1. Read the .cast file content
2. Parse JSON lines - extract output events (type "o")
3. Identify key moments:
   - Errors, exceptions, stack traces
   - Important commands being executed
   - Decision points or turning points
   - Significant output or results
4. For each moment, call:
   ```bash
   asr marker add <file.cast> <timestamp_seconds> "description"
   ```

**Example:**
```bash
# Found error at 45.2 seconds
asr marker add session.cast 45.2 "Build failed: missing dependency"

# Found successful deployment at 120.5 seconds
asr marker add session.cast 120.5 "Deployment completed successfully"
```

### `/asr-review <pr-number>`

Review a pull request for this project.

**How to use:**
1. Fetch PR details: `gh pr view <number>`
2. Get diff: `gh pr diff <number>`
3. Check CI status: `gh pr checks <number>`
4. Review for:
   - Code correctness
   - Error handling
   - Test coverage
   - Security issues
5. Post review: `gh pr review <number> --approve` or `--request-changes`

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

**See `.state/plan.md` for current phase status and task lists.**

Summary:
- **Phase 1:** COMPLETE - Core recording functionality
- **Phase 2:** IN PROGRESS - Storage UX improvements
- **Phase 3:** MOSTLY COMPLETE - Marker support
- **Phase 4:** MOSTLY COMPLETE - Polish & distribution

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
