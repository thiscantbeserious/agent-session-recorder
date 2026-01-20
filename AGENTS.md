# Agent Session Recorder (AGR)

A Rust CLI tool for recording AI agent terminal sessions with asciinema.

## IMPORTANT: Read Before Any Implementation

**You are a COORDINATOR.** Before implementing anything, read `architecture/AGENTS.md`.

You MUST spawn sub-agents for implementation work - never implement directly.

## Documentation

| Doc | Purpose |
|-----|---------|
| `architecture/AGENTS.md` | **READ FIRST** - Orchestrator pattern, coordinator rules |
| `.state/INDEX.md` | Current state, where to find things |
| `.state/decisions.md` | Technical decisions log |
| `.state-templates/` | Templates for state files |

## Before Starting Work

```bash
# 1. Check current state
cat .state/INDEX.md
gh pr list                       # Open PRs
gh pr list --state merged -L 10  # Recent completed work

# 2. Check decisions
cat .state/decisions.md
```

### For Each Task, Follow These Steps:

#### 1. Requirement Gathering
```bash
gh pr list --state merged        # What's been done
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
**MANDATORY: Wait for CI AND CodeRabbit to pass before merging!**

1. Create PR: `gh pr create --title "feat(scope): description"`
2. Wait for all checks to complete:
   ```bash
   gh pr checks <PR_NUMBER>   # Check CI status
   gh pr view <PR_NUMBER> --comments   # Check CodeRabbit review
   ```
3. **CI Pipeline stages** (must ALL pass):
   - **Build** - compilation on ubuntu + macos
   - **Unit Tests** - cargo test on both OS
   - **E2E Tests** - integration tests (macOS only)
   - **Lint** - cargo fmt + clippy
4. **CodeRabbit Review** (must complete):
   - Wait for CodeRabbit to post actual review (not just "processing")
   - Review any issues CodeRabbit identifies
   - **VERIFY SUGGESTIONS LOCALLY** before implementing:
     - For CLI tool syntax: run `<tool> --help` to check actual interface
     - For API changes: check actual code/docs, not just CodeRabbit's claim
     - CodeRabbit may have outdated info about third-party tools
   - **When fixing issues:** Look for the **🤖 Prompt for AI Agents** section in CodeRabbit's comments - it contains ready-to-use code snippets and instructions for implementing the suggested fix
   - Fix blocking issues before merge
5. **NEVER merge until:**
   - ALL CI checks show `pass`
   - CodeRabbit review is complete (not "processing")
   - No blocking issues from CodeRabbit
6. If checks fail, fix issues and push again
7. Merge command: `gh pr merge <NUMBER> --squash --delete-branch`

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
- AI agents analyze recordings and add markers via `/agr-analyze` skill

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
agr record <agent> [-- args...]   # Start recording session
agr status                        # Show storage stats
agr cleanup                       # Interactive cleanup
agr list [agent]                  # List sessions
agr marker add <file> <time> <label>  # Add marker
agr marker list <file>            # List markers
agr agents list                   # List configured agents
agr agents add <name>             # Add agent to config
agr config show                   # Show current config
agr config edit                   # Open config in editor
```

## Auto-Analysis

When `auto_analyze = true` in config, AGR automatically spawns an AI agent after each recording to analyze the session and add markers.

**Config:**
```toml
[recording]
auto_analyze = true
analysis_agent = "claude"  # or "codex" or "gemini-cli"
```

**What the analyzer does:**
1. Reads the .cast file
2. Identifies key moments (errors, commands, decisions, results)
3. Adds markers via `agr marker add <file> <time> "description"`

**Manual analysis** (if auto_analyze is disabled):
```bash
# Read the cast file and identify interesting moments, then:
agr marker add session.cast 45.2 "Build failed: missing dependency"
agr marker add session.cast 120.5 "Deployment completed successfully"
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
