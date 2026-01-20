# Agent Session Recorder (ASR)

## Overview
A small command-line tool that uses asciinema to track all AI agent sessions, leveraging the agents themselves to create markers at interesting key points automatically, in addition to keeping track of total usage.

**Key design principles:**
- Single binary, no runtime dependencies
- Transparent shell wrappers - doesn't change your workflow
- AI agents analyze their own recordings and add markers
- Native asciicast v3 format for compatibility

---

## Multi-Agent Coordination Architecture

### Session Roles

| Role | Responsibility | Spawning |
|------|----------------|----------|
| **Coordinator** | Orchestrates all work, never implements directly | Main session (this one) |
| **Impl Agent** | Implements features on feature branches | Spawned per task/phase |
| **Verify Agent** | Fresh session to verify work, run tests, review PRs | Spawned after impl completes |

### Coordinator Responsibilities

The coordinator session (this session) NEVER writes code directly. Instead it:

1. **Plans work** - Breaks phases into discrete tasks
2. **Spawns Impl Agents** - One per task, runs in background
3. **Monitors progress** - Checks state files for completion
4. **Spawns Verify Agents** - Fresh session to validate implementation
5. **Gates merges** - Only merges PRs after verification passes
6. **Handles conflicts** - Resolves issues between agents

### Workflow Diagram (Sequential with Feedback Loop)

```
┌─────────────────────────────────────────────────────────────────┐
│                     COORDINATOR SESSION                          │
│  (Plans, spawns agents, verifies, never implements directly)     │
└─────────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         │                    ▼                    │
         │          ┌─────────────────┐            │
         │          │  IMPL AGENT     │  ◄── Fresh session
         │          │  (Task N)       │            │
         │          │  feature/...    │            │
         │          └────────┬────────┘            │
         │                   │ PR created          │
         │                   ▼                     │
         │          ┌─────────────────┐            │
         │          │  CODERABBIT     │  ◄── External (auto-triggers)
         │          │  (GitHub App)   │            │
         │          │  AI code review │            │
         │          └────────┬────────┘            │
         │                   │ Review posted       │
         │                   ▼                     │
         │          ┌─────────────────┐            │
         │          │  VERIFY AGENT   │  ◄── Fresh session
         │          │  - cargo test   │            │
         │          │  - e2e tests    │            │
         │          │  - Check CR     │  ◄── Read CodeRabbit feedback
         │          └────────┬────────┘            │
         │                   │                     │
         │                   ▼                     │
         │  ┌─────────────────────────────────┐    │
         │  │      COORDINATOR FEEDBACK       │    │
         │  │  - Review verify agent results  │    │
         │  │  - Update decisions.md          │    │
         │  │  - Adjust future task approach  │    │
         │  │  - Document lessons learned     │    │
         │  └────────────────┬────────────────┘    │
         │                   │                     │
         │         ┌─────────┴─────────┐           │
         │         │                   │           │
         │         ▼                   ▼           │
         │   ┌──────────┐        ┌──────────┐      │
         │   │  PASS    │        │  FAIL    │      │
         │   └────┬─────┘        └────┬─────┘      │
         │        │                   │            │
         │        ▼                   ▼            │
         │   Merge PR            Iterate:          │
         │        │              - Document issue  │
         │        │              - Update approach │
         │        │              - Spawn new Impl  │
         │        │                   │            │
         │        │                   └────────────┘
         │        ▼                   (feedback informs retry)
         │  ┌─────────────┐
         └──│  Task N+1   │
            │  (informed  │
            │  by prior   │
            │  learnings) │
            └─────────────┘
```

**Agile iteration principles:**
- Every verification produces feedback (pass OR fail)
- Coordinator documents learnings in `.state/decisions.md`
- Future tasks are informed by prior iteration results
- Failures trigger reflection, not just mechanical fixes
- Continuous improvement through each cycle

**Sequential execution rules:**
- Only ONE impl agent active at a time
- Wait for verification AND feedback before next task
- Each task builds on merged main + accumulated learnings
- No parallel task execution (avoids merge conflicts)

### CodeRabbit Integration

CodeRabbit is an external AI code review tool that auto-reviews PRs on GitHub.

**⚠️ STRICT RULE: NEVER merge a PR until CodeRabbit review is complete!**

**How it fits in the workflow:**
1. Impl Agent creates PR → CodeRabbit automatically triggered
2. CodeRabbit posts review comments on the PR (async, may take several minutes)
3. Verify Agent checks CodeRabbit feedback via `gh pr view --comments`
4. **WAIT** until CodeRabbit posts actual review (not just "processing")
5. Coordinator considers both Verify Agent + CodeRabbit results
6. **ONLY THEN** can Coordinator merge

**CodeRabbit provides:**
- AI-powered code review (different perspective than our agents)
- Security vulnerability detection
- Code quality suggestions
- Bug detection

**Verify Agent must:**
- Wait for CodeRabbit review before completing verification
- If CodeRabbit shows "processing", report PENDING and DO NOT approve
- Include CodeRabbit findings in report to coordinator
- Flag any blocking issues CodeRabbit identified

**Coordinator MUST:**
- NEVER merge if CodeRabbit review is still "processing"
- Wait and re-check CodeRabbit status before merging
- Only merge after CodeRabbit has posted actual review findings

### Task Agent Prompts

**Impl Agent Prompt Template:**
```
You are implementing a specific task for the ASR project.

Project: ~/git/simon/agent-session-recorder/
Task: [TASK DESCRIPTION]
Branch: feature/phase[N]-[task-name]

Instructions:
1. Read .state/current-phase.md and .state/decisions.md for context
2. Create feature branch from main
3. Implement the task following TDD
4. Run cargo test to verify
5. Commit changes with conventional commit message
6. Push branch and create PR
7. Update .state/phase-[N]/progress.md with results

DO NOT merge the PR - the coordinator will handle that after verification.
```

**Verify Agent Prompt Template:**
```
You are verifying implementation work for the ASR project.

Project: ~/git/simon/agent-session-recorder/
PR: #[NUMBER]
Branch: feature/phase[N]-[task-name]

Instructions:
1. git fetch && git checkout [branch]
2. Run: cargo test
3. Run: ./tests/e2e_test.sh
4. **CodeRabbit Verification (MANDATORY):**
   a. Check CodeRabbit status: gh pr view [NUMBER] --comments
   b. Look for CodeRabbit's review comment (NOT just "processing")
   c. If CodeRabbit shows "processing" or "analyzing" → WAIT and re-check
   d. If CodeRabbit completed:
      - Note all issues/suggestions it identified
      - Verify each suggestion against the actual code
   e. If CodeRabbit skipped (e.g., docs-only PR due to path_filters):
      - This is OK for documentation-only changes
      - Report "CodeRabbit: skipped (docs-only)" in your verification
      - Perform extra careful manual review for docs changes
5. Review the PR diff: gh pr diff [NUMBER]
6. Check for:
   - All tests pass
   - CodeRabbit has no blocking issues (or was appropriately skipped)
   - No obvious bugs
   - Follows existing patterns
   - No security issues
   - Documentation changes are accurate and helpful (if applicable)
7. Report results to coordinator:
   - PASS: All checks passed (tests + CodeRabbit reviewed/skipped appropriately)
   - PENDING: CodeRabbit still processing (DO NOT proceed)
   - FAIL: [specific issues found, include CodeRabbit feedback]

DO NOT merge or approve the PR - just report findings.
CRITICAL: Never report PASS if CodeRabbit is still "processing"!
```

### Fresh Session Policy

Sub-agents are spawned fresh for each task to:
- Avoid context pollution between tasks
- Ensure clean verification (no cached assumptions)
- Prevent cascading errors from previous work
- Keep context windows manageable

### State File Protocol

All agents communicate through state files:

```
.state/
├── coordinator.md          # Coordinator's current plan and status
├── current-phase.md        # Overall phase status
├── decisions.md            # Shared decisions log
├── locks/                  # Task claiming (one agent per task)
│   └── task-name.lock
└── phase-N/
    ├── progress.md         # Task checklist with agent results
    ├── impl-results/       # Impl agent outputs
    │   └── task-name.md
    └── verify-results/     # Verify agent outputs
        └── task-name.md
```

### Error Handling

1. **Impl Agent Fails**: Coordinator reviews failure, may spawn new impl agent with additional context
2. **Verify Agent Fails**: Coordinator spawns new impl agent to fix issues, then re-verifies
3. **Conflict**: Coordinator resolves manually, documents in decisions.md

---

## Core Architecture

**Separation of concerns:**
- **Rust binary** = fast, mechanical operations (recording, storage, marker injection)
- **Agent skills** = intelligent analysis (finding interesting moments in sessions)

## Key Insight
- asciicast v3 already has **native marker support** (`"m"` events)
- We inject markers directly into `.cast` files, not separate files
- AI agents analyze content and tell the CLI where to add markers

## Language Choice: Rust

**Rationale:**
- Single static binary, zero runtime dependencies
- Fast execution
- Easy cross-compilation (macOS arm64/x86, Linux)
- Good CLI ecosystem (`clap`, `serde_json`)
- Once stable, minimal changes needed

**Note:** The `asciinema` crate is just the CLI binary, not a library. We shell out to `asciinema rec` for recording, but handle asciicast file parsing/writing natively with `serde_json`.

**Key dependencies:**
- `clap` - CLI argument parsing
- `serde` + `serde_json` - asciicast v3 parsing/writing
- `ctrlc` - signal handling for graceful interrupt
- `dirs` - cross-platform home directory
- `humansize` - human-readable file sizes
- `toml` - config file parsing

**Dev dependencies:**
- `tempfile` - temporary directories for tests
- `assert_cmd` - CLI integration testing
- `predicates` - test assertions

**Coverage tool:**
- `cargo-tarpaulin` - code coverage (installed in Docker)

## asciicast v3 Reference

- **Format spec**: https://docs.asciinema.org/manual/asciicast/v3/
- **Rust implementation**: https://github.com/asciinema/asciinema/blob/develop/src/asciicast/v3.rs

Use these as reference when implementing `src/asciicast.rs`. Key: marker events use code `'m'` with label as data.

## Project Structure

```
agent-session-recorder/
├── AGENTS.md                     # Main agent instructions (includes workflow protocol)
├── CLAUDE.md -> AGENTS.md        # Symlink
├── GEMINI.md -> AGENTS.md        # Symlink
├── .state/                       # Persistent state for agent continuity
│   ├── current-phase.md          # Current task & context
│   ├── decisions.md              # Key decisions log
│   ├── locks/                    # Task claim locks (race condition protection)
│   ├── phase-1/
│   │   ├── progress.md           # Task checklist
│   │   ├── blockers.md           # Issues encountered
│   │   └── test-results.md       # Verification output
│   └── phase-2/...
├── agents/                       # Skills folder (symlinked to ~/.claude/commands/ etc)
│   └── asr-analyze.md            # Skill: AI reads .cast, calls CLI to add markers
├── src/
│   ├── main.rs                   # CLI entry point (clap)
│   ├── lib.rs                    # Library root (for testing)
│   ├── recording.rs              # Recording logic (wraps asciinema)
│   ├── asciicast.rs              # asciicast v3 parser/writer
│   ├── markers.rs                # Marker injection
│   ├── storage.rs                # Storage stats & cleanup
│   └── config.rs                 # Configuration
├── tests/                        # Integration tests
│   ├── recording_test.rs         # Recording behavior tests
│   ├── asciicast_test.rs         # Parser/writer tests
│   ├── markers_test.rs           # Marker injection tests
│   ├── storage_test.rs           # Storage management tests
│   ├── fixtures/                 # Test data
│   │   ├── sample.cast           # Sample asciicast file
│   │   └── with_markers.cast     # File with existing markers
│   └── helpers/
│       └── mod.rs                # Test utilities
├── shell/
│   └── asr.sh                    # Thin shell wrapper (sourced by .zshrc)
├── docker/
│   ├── Dockerfile                # Build environment
│   └── docker-compose.yml        # Build orchestration
├── Cargo.toml
├── build.sh                      # Docker-based build script
├── install.sh                    # Installer (uses pre-built binary)
├── uninstall.sh
└── README.md
```

## Docker Build Environment

All builds happen in Docker for reproducibility. No Rust toolchain needed on host.

**`docker/Dockerfile`:**
```dockerfile
FROM rust:1.75-slim AS base
WORKDIR /app
RUN cargo install cargo-tarpaulin

FROM base AS test
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
RUN cargo test
RUN cargo tarpaulin --fail-under 90 --out Html --output-dir /coverage

FROM base AS builder
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM scratch AS export
COPY --from=builder /app/target/release/asr /asr
```

**`build.sh` updated to run tests:**
```bash
#!/usr/bin/env bash
set -e

# Run tests first
echo "Running tests..."
docker build -f docker/Dockerfile --target test .

# Then build release
echo "Building release..."
docker build -f docker/Dockerfile --target export -o dist/ .
```

**`install.sh`** now:
1. Runs `./build.sh` if no pre-built binary
2. Copies binary from `dist/` to `~/.local/bin/asr`
3. Sets up shell integration

## Agent Integration

### Symlinks for Agent Instructions
```
AGENTS.md           # Main file with project context
CLAUDE.md -> AGENTS.md
GEMINI.md -> AGENTS.md
```

### The Analysis Skill (`/asr-analyze`)

This is where AI shines - semantic understanding of session content.

**`agents/asr-analyze.md`:**
```markdown
# Analyze Session Recording

Analyze the specified .cast file and add markers for interesting moments.

## Usage
/asr-analyze <path-to-file.cast>

## Process
1. Read the .cast file using the Read tool
2. Parse the JSON lines to extract terminal output (events with type "o")
3. Identify key moments:
   - Errors, exceptions, or failures
   - Important commands being executed
   - Decisions or turning points
   - Significant output or results
4. For each key moment, run:
   ```
   asr marker add <file.cast> <timestamp_seconds> "description"
   ```

## Example
If you find an error at timestamp 45.2s:
```bash
asr marker add session.cast 45.2 "Build failed: missing dependency"
```
```

### Skill Installation
After `./install.sh`:
```
~/.claude/commands/asr-analyze.md -> <project>/agents/asr-analyze.md
~/.codex/commands/asr-analyze.md -> <project>/agents/asr-analyze.md
~/.gemini/commands/asr-analyze.md -> <project>/agents/asr-analyze.md
```

This allows invoking `/asr-analyze session.cast` in any agent session.

## Shell Integration (Thin Wrapper)

```bash
# shell/asr.sh - sourced by .zshrc/.bashrc

# Only wrap if asciinema available and not already recording
_record_session() {
  local agent="$1"; shift
  if [[ -z "${ASCIINEMA_REC:-}" ]] && command -v asciinema &>/dev/null; then
    asr record "$agent" -- "$@"
  else
    command "$agent" "$@"
  fi
}

# Generate wrappers from config
for agent in $(asr agents list 2>/dev/null); do
  eval "$agent() { _record_session $agent \"\$@\"; }"
done
```

## CLI Commands

```
asr record <agent> [-- args...]   # Start recording session
asr status                        # Show storage stats (size, %, cleanup hint)
asr cleanup                       # Interactive cleanup (select count to delete)
asr list [agent]                  # List sessions (optionally filter by agent)
asr marker add <file> <time> <label>  # Add marker to .cast file
asr marker list <file>            # List markers in .cast file
asr agents list                   # List configured agents
asr agents add <name>             # Add agent to config
asr config show                   # Show current config
asr config edit                   # Open config in editor
```

**Note:** No `asr analyze` command - that's handled by the agent skill which calls `asr marker add`.

## Core Features

### 1. Recording (`asr record`)

Flow:
1. Create session directory: `~/recorded_agent_sessions/<agent>/`
2. Generate timestamp filename: `20250119-143052-12345.cast`
3. Spawn asciinema: `asciinema rec <file> --title "<agent> session" -c "<agent> <args>"`
4. On normal exit: prompt for rename, sanitize input, rename file
5. On interrupt (Ctrl+C): keep original filename
6. Show storage stats hint if threshold exceeded

### 2. Marker Injection (`asr marker add`)

Uses native asciicast v3 marker format. The CLI:
1. Parses the .cast file (newline-delimited JSON)
2. Calculates where to insert based on cumulative timestamps
3. Inserts marker event: `[<interval>, "m", "<label>"]`
4. Rewrites the file

**Example:**
```bash
$ asr marker add session.cast 45.2 "Build error: missing dep"
Marker added at 45.2s: "Build error: missing dep"
```

The AI skill calls this command after analyzing the session content.

### 3. Storage Management (`asr status`, `asr cleanup`)

**`asr status` output:**
```
📁 Agent Sessions: 2.3 GB (1.2% of disk)
   Sessions: 47 total (claude: 32, codex: 12, gemini-cli: 3)
   Oldest: 2024-12-15 (35 days ago)
```

**`asr cleanup` interactive flow:**
```
=== Agent Session Cleanup ===
Storage: 2.3 GB (1.2% of disk)

Found 47 sessions. Oldest 10:
  1) 20241215-091532.cast (claude, 45 MB, 35 days)
  2) 20241218-142311.cast (codex, 23 MB, 32 days)
  ...

How many oldest to delete? [0-47]: 5

Will delete:
  - 20241215-091532.cast
  - 20241218-142311.cast
  - ...

Confirm? [y/N]: y
Deleted 5 sessions. New size: 1.9 GB
```

## Configuration

`~/.config/asr/config.toml`:
```toml
[storage]
directory = "~/recorded_agent_sessions"
size_threshold_gb = 5
age_threshold_days = 30

[agents]
enabled = ["claude", "codex", "gemini-cli"]
```

## Installation

```bash
# Clone & install
git clone <repo>
cd agent-session-recorder
./install.sh
```

**`install.sh` steps:**
1. Detect OS (macOS/Linux)
2. Install asciinema if missing (brew/apt/cargo)
3. Build Rust binary: `cargo build --release`
4. Copy binary to `~/.local/bin/asr`
5. Create session directory: `~/recorded_agent_sessions/`
6. Create config directory: `~/.config/asr/`
7. Symlink skills to agent command directories
8. Add shell sourcing to `.zshrc`/`.bashrc`
9. Remove old `_ai_wrap` code if present

## AGENTS.md Content (Draft)

```markdown
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
- All tests must pass: `cargo test`
- Coverage must be ≥90%: `cargo tarpaulin --fail-under 90`
- Run verification command (see table below)
- Log results to `.state/phase-N/test-results.md`

#### 5. Deployment
- Build with `./build.sh`
- If tests pass: mark `[x]` in `progress.md`
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
- `src/main.rs` - CLI entry point (clap)
- `src/recording.rs` - asciinema wrapper, rename flow
- `src/asciicast.rs` - v3 format parser/writer
- `src/markers.rs` - marker injection logic
- `src/storage.rs` - storage stats & cleanup

## Verification Commands

| Task | Test Command |
|------|--------------|
| Docker build | `./build.sh && ls dist/` |
| `asr record` | `./dist/asr record echo "test"` |
| `asr agents` | `./dist/asr agents list` |
| `asr status` | `./dist/asr status` |
| `asr marker` | `./dist/asr marker add <file> 1.0 "test"` |

## Reference
- asciicast v3 spec: https://docs.asciinema.org/manual/asciicast/v3/
- Rust impl: https://github.com/asciinema/asciinema/blob/develop/src/asciicast/v3.rs

## Building
```bash
./build.sh            # Docker build
./install.sh          # Install to system
```
```

## Files to Create

| File | Purpose |
|------|---------|
| `AGENTS.md` | Main agent instructions with workflow protocol |
| `CLAUDE.md` | Symlink → AGENTS.md |
| `GEMINI.md` | Symlink → AGENTS.md |
| `.state/current-phase.md` | Current task & context (agent state) |
| `.state/decisions.md` | Key decisions log |
| `.state/locks/` | Task claim locks directory |
| `.state/phase-1/progress.md` | Phase 1 task checklist |
| `.state/phase-1/blockers.md` | Phase 1 issues |
| `.state/phase-1/test-results.md` | Phase 1 test output |
| `agents/asr-analyze.md` | Skill: AI analyzes session, calls marker add |
| `agents/asr-review.md` | Skill: Code review for PRs |
| `LICENSE` | MIT license |
| `src/main.rs` | CLI entry (clap subcommands) |
| `src/lib.rs` | Library root for testability |
| `src/recording.rs` | Recording logic (spawn asciinema, rename flow) |
| `src/asciicast.rs` | asciicast v3 parser/writer |
| `src/markers.rs` | Marker injection |
| `src/storage.rs` | Storage stats & cleanup |
| `src/config.rs` | Config loading (TOML) |
| `tests/asciicast_test.rs` | Asciicast parser/writer behavior tests |
| `tests/markers_test.rs` | Marker injection behavior tests |
| `tests/storage_test.rs` | Storage management behavior tests |
| `tests/fixtures/sample.cast` | Test fixture: basic asciicast |
| `tests/fixtures/with_markers.cast` | Test fixture: file with markers |
| `shell/asr.sh` | Thin shell wrapper |
| `docker/Dockerfile` | Multi-stage build |
| `docker/docker-compose.yml` | Build orchestration |
| `Cargo.toml` | Rust package config |
| `build.sh` | Docker-based build script |
| `install.sh` | Installer script |
| `uninstall.sh` | Uninstaller script |
| `README.md` | Documentation |

## Verification Plan

1. **Docker build**: Run `./build.sh`, verify binary created in `dist/`
2. **Install**: Run `./install.sh`, verify binary in PATH, skills symlinked
3. **Record**: Run `claude --help`, verify `.cast` created in `~/recorded_agent_sessions/claude/`
4. **Rename**: Exit session normally, enter a name, verify file renamed
5. **Interrupt**: Start session, Ctrl+C, verify original timestamp filename kept
6. **Marker add**: Run `asr marker add <file> 10.5 "test"`, verify marker in file
7. **Status**: Run `asr status`, verify human-readable output with percentage
8. **Cleanup**: Run `asr cleanup`, test selecting 2 files, confirm, verify deleted
9. **Skill test**: In a Claude session, run `/asr-analyze <file.cast>`, verify markers added

## Project Location

`~/git/simon/agent-session-recorder/`

## Git Workflow

### Repository Setup
- Initialize Git repo with `main` branch
- Add **MIT license**
- Create GitHub repository: `github.com/simon/agent-session-recorder`
- Push initial structure

### Feature Branch Strategy

Each task gets its own feature branch:
```
main
├── feature/phase1-project-setup
├── feature/phase1-docker-build
├── feature/phase1-asr-record
├── feature/phase1-rename-prompt
├── feature/phase1-ctrl-c-handling
├── feature/phase1-agents-command
├── feature/phase1-shell-wrapper
├── feature/phase1-install-script
├── feature/phase2-status
├── feature/phase2-cleanup
...
```

### Branch Naming Convention
```
feature/phase<N>-<task-name>
```

### Commit Message Format
```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

**Examples:**
```
feat(recording): add asciinema subprocess spawning

- Spawn asciinema rec with proper arguments
- Handle session directory creation
- Generate timestamp-based filenames

Closes #1
```

### Pull Request Workflow

1. **Create branch** from `main`:
   ```bash
   git checkout -b feature/phase1-asr-record
   ```

2. **Implement** the task with commits

3. **Push** and create PR:
   ```bash
   git push -u origin feature/phase1-asr-record
   gh pr create --title "feat(recording): implement asr record command" \
     --body "## Summary\n- Implements basic recording\n\n## Test Plan\n- Run \`asr record echo test\`"
   ```

4. **Code Review** by review agent (see below)

5. **Merge** after approval:
   ```bash
   gh pr merge --squash
   ```

### Code Review Agent

A separate agent reviews PRs before merge.

**Review skill (`agents/asr-review.md`):**
```markdown
# Code Review

Review the specified PR for quality and correctness.

## Usage
/asr-review <pr-number>

## Process
1. Fetch PR diff: `gh pr diff <pr-number>`
2. Read changed files
3. Check for:
   - Code correctness
   - Error handling
   - Edge cases
   - Style consistency
   - Security issues
4. Post review:
   ```bash
   gh pr review <pr-number> --approve  # or --request-changes
   gh pr review <pr-number> --comment --body "..."
   ```

## Review Checklist
- [ ] Tests exist and follow TDD (written before code)
- [ ] Tests are behavior-focused, not implementation-focused
- [ ] All tests pass (`cargo test`)
- [ ] Coverage ≥90% (`cargo tarpaulin --fail-under 90`)
- [ ] Code compiles (`./build.sh`)
- [ ] No obvious bugs
- [ ] Error handling present
- [ ] Consistent style
- [ ] No security issues
```

### Agent Roles

| Agent | Role |
|-------|------|
| **Main Agent** | Coordinates phases, assigns tasks |
| **Impl Subagent(s)** | Implements tasks on feature branches |
| **Review Agent** | Reviews PRs, approves or requests changes |

### Workflow Diagram
```
Main Agent
    │
    ├─► Impl Agent 1 ─► feature/task-a ─► PR #1 ─┐
    │                                             │
    ├─► Impl Agent 2 ─► feature/task-b ─► PR #2 ─┼─► Review Agent
    │                                             │       │
    ├─► Impl Agent 3 ─► feature/task-c ─► PR #3 ─┘       │
    │                                                     │
    └─────────────────── merge to main ◄─────────────────┘
```

## Test-Driven Development (TDD)

### Coverage Target: 90%

All code must be tested with **behavior-focused tests**, not implementation tests.

### TDD Workflow (Red-Green-Refactor)

For each feature:

1. **RED**: Write a failing test that describes the desired behavior
2. **GREEN**: Write minimal code to make the test pass
3. **REFACTOR**: Clean up code while keeping tests green

```
┌─────────────────────────────────────────────┐
│  1. Write failing test (behavior spec)       │
│              ↓                               │
│  2. Run test → FAILS (red)                   │
│              ↓                               │
│  3. Write minimal implementation             │
│              ↓                               │
│  4. Run test → PASSES (green)                │
│              ↓                               │
│  5. Refactor if needed                       │
│              ↓                               │
│  6. Run all tests → PASSES                   │
│              ↓                               │
│  7. Commit                                   │
└─────────────────────────────────────────────┘
```

### Test Categories

| Category | Location | Purpose |
|----------|----------|---------|
| **Unit tests** | `src/*.rs` (inline) | Test individual functions |
| **Integration tests** | `tests/*.rs` | Test module interactions |
| **Behavior tests** | `tests/*.rs` | Test user-facing features |

### Behavior-Focused Testing

Tests should describe **what** the system does, not **how** it does it.

**BAD (tests implementation):**
```rust
#[test]
fn test_internal_parse_line() {
    let result = parse_line("[0.5, \"o\", \"hello\"]");
    assert_eq!(result.time, 0.5);
}
```

**GOOD (tests behavior):**
```rust
#[test]
fn asciicast_parser_extracts_output_events() {
    let cast = load_fixture("sample.cast");
    let events = parse_asciicast(&cast);

    let output_events: Vec<_> = events.iter()
        .filter(|e| e.is_output())
        .collect();

    assert!(!output_events.is_empty());
    assert!(output_events[0].data.contains("expected text"));
}

#[test]
fn marker_injection_preserves_existing_events() {
    let original = load_fixture("sample.cast");
    let original_event_count = count_events(&original);

    add_marker(&original, 10.0, "test marker");

    let modified = read_cast(&original);
    assert_eq!(count_events(&modified), original_event_count + 1);
}
```

### Test Requirements per Module

| Module | Must Test |
|--------|-----------|
| `asciicast.rs` | Parse valid v3 files, handle malformed input, preserve data on roundtrip |
| `markers.rs` | Insert at correct position, preserve existing markers, handle edge cases (start/end) |
| `storage.rs` | Calculate sizes correctly, identify oldest files, handle empty directories |
| `config.rs` | Load valid config, provide defaults for missing values, reject invalid config |
| `recording.rs` | Generate correct filenames, handle missing asciinema, create directories |

### Running Tests

```bash
# Run all tests
cargo test

# Run with coverage (requires cargo-tarpaulin)
cargo tarpaulin --out Html --output-dir coverage/

# Run specific test
cargo test asciicast_parser

# Run tests in Docker
docker run --rm -v $(pwd):/app -w /app rust:1.75 cargo test
```

### Coverage Enforcement

The Dockerfile includes coverage check:
```dockerfile
# In CI/test stage
RUN cargo tarpaulin --out Xml --fail-under 90
```

PRs that drop coverage below 90% will fail review.

### Test Fixtures

`tests/fixtures/sample.cast`:
```json
{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","$ echo hello\r\n"]
[0.1,"o","hello\r\n"]
[0.2,"o","$ "]
```

`tests/fixtures/with_markers.cast`:
```json
{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","$ make build\r\n"]
[1.0,"m","Build started"]
[2.5,"o","Build complete\r\n"]
[0.1,"m","Build finished"]
```

## Agile Development Approach

We'll use an iterative approach with working software at each phase. Each phase produces a usable tool.

### State Management for Agent Continuity

State is persisted in markdown files so agents can pick up where previous sessions left off.

**State directory structure:**
```
.state/
├── current-phase.md      # Which phase we're on, what task is in progress
├── locks/                # Task claim locks (race condition protection)
│   └── <task-id>.lock    # Contains agent ID + timestamp
├── phase-1/
│   ├── progress.md       # Checklist with status of each task
│   ├── blockers.md       # Any issues encountered
│   └── test-results.md   # Output from verification tests
├── phase-2/
│   └── ...
└── decisions.md          # Key decisions made during development
```

### Race Condition Protection for Subagents

When multiple subagents work in parallel, they must claim tasks to avoid conflicts.

**Task Claiming Protocol:**

1. **Before starting a task**, create a lock file:
   ```bash
   mkdir -p .state/locks
   echo "agent-$(date +%s)-$$" > .state/locks/task-name.lock
   ```

2. **Check if task is already claimed**:
   ```bash
   if [ -f .state/locks/task-name.lock ]; then
     echo "Task already claimed, skipping"
     # Pick a different unclaimed task
   fi
   ```

3. **On task completion**, remove lock:
   ```bash
   rm .state/locks/task-name.lock
   ```

**Rules for Parallel Subagents:**
- Each subagent claims ONE task at a time
- Never work on a task that has a lock file
- Tasks are independent within a phase (no dependencies)
- Sequential tasks (across phases) must wait for previous phase completion
- If a lock is stale (>1 hour old), it can be cleared

**Recommended Subagent Strategy:**
- Use Claude subagents for parallel independent tasks
- Main agent coordinates phases (waits for all Phase 1 tasks before Phase 2)
- Each subagent reports results to state files
- Main agent validates and merges results

**`current-phase.md` format:**
```markdown
# Current State

Phase: 1
Task: `asr record <agent>` implementation
Status: in_progress
Last Updated: 2025-01-19T14:30:00

## Context
Working on spawning asciinema subprocess and capturing exit status.

## Next Steps
1. Handle SIGINT signal
2. Test with real claude session
```

**`progress.md` format (initial state):**
```markdown
# Phase 1 Progress

- [ ] Project setup (Cargo.toml, basic structure)
- [ ] Docker build environment (Dockerfile, build.sh)
- [ ] `asr record <agent>` - spawn asciinema, save to session dir
- [ ] Rename prompt on normal exit
- [ ] Keep original filename on Ctrl+C
- [ ] `asr agents list/add`
- [ ] Shell wrapper (`asr.sh`)
- [ ] Basic install script
```

Tasks are marked `[x]` when completed, `- IN PROGRESS` suffix when being worked on.

### Agile SDLC Workflow for Each Task

Each task follows the full Agile SDLC cycle:

#### 1. Requirement Gathering
- Read `.state/current-phase.md` for context
- Read `.state/phase-N/progress.md` to identify next task
- Understand what the task needs to accomplish
- Check `.state/decisions.md` for relevant prior decisions

#### 2. Design
- Break task into smaller implementation steps if needed
- Identify which files need to be created/modified
- Consider edge cases and error handling
- Document design decisions in `.state/decisions.md`

#### 3. Coding (Sprint)
- Implement the solution in small increments
- Commit frequently with clear messages
- Follow existing code patterns and style
- Keep changes focused on the current task

#### 4. Testing / QA
- Run the task's verification command (see table below)
- Write unit tests if applicable
- Manual testing for interactive features
- Log results to `.state/phase-N/test-results.md`

#### 5. Deployment
- Build with `./build.sh`
- If task passes tests, mark `[x]` in progress.md
- Update `.state/current-phase.md` with next task
- Commit state changes

#### 6. Feedback
- Document any issues in `.state/phase-N/blockers.md`
- Note improvements for future iterations
- If blocked, document the blocker and move to next task
- Update `.state/decisions.md` with lessons learned

### Verification After Each Task

After completing a task, run its specific test:

| Task | Verification Command |
|------|---------------------|
| Docker build | `./build.sh && ls dist/asr-*` |
| `asr record` | `./dist/asr record echo "test" && ls ~/recorded_agent_sessions/` |
| Rename prompt | Manual: exit session, type name, check file |
| Ctrl+C handling | Manual: start session, Ctrl+C, check filename |
| `asr agents` | `./dist/asr agents list` |
| Shell wrapper | `source shell/asr.sh && type claude` |
| Install script | `./install.sh && which asr` |

Log results to `.state/phase-N/test-results.md`

### Phase 1: MVP (Core Recording) - COMPLETE
**Goal:** Replace current shell script with working Rust binary

- [x] Project setup (Cargo.toml, basic structure)
- [x] Docker build environment (Dockerfile, build.sh)
- [x] `asr record <agent>` - spawn asciinema, save to session dir
- [x] Rename prompt on normal exit
- [x] Keep original filename on Ctrl+C
- [x] `asr agents list/add` - manage agent config
- [x] Shell wrapper (`asr.sh`)
- [x] Basic install script

**Definition of Done:** ✅ All complete. PR #1 merged.

### Phase 2: Storage Management - COMPLETE ✅
**Goal:** Improve storage visibility and cleanup UX

- [x] `asr status` - basic implementation exists
- [x] `asr cleanup` - basic implementation exists
- [x] `asr list` - basic implementation exists
- [x] Improve `asr status` output (breakdown by agent, disk %) - PR #4
- [x] Enhance `asr cleanup` UX (filtering, formatting, UTF-8 safe) - PR #5
- [x] Add storage threshold warnings after recording (already implemented in Phase 1)
- [x] Improve `asr list` output (table formatting, summary) - PR #6

**Definition of Done:** ✅ All complete. Polished storage commands with better UX.

### Phase 3: Marker Support - COMPLETE ✅
**Goal:** AI-powered session analysis

- [x] `asr marker add <file> <time> <label>` - inject marker
- [x] `asr marker list <file>` - show markers
- [x] asciicast v3 parser/writer module
- [x] `/asr-analyze` skill documentation (in AGENTS.md)
- [x] Skill symlink setup in installer (agents/asr-analyze.md, asr-review.md)

**Definition of Done:** ✅ Can run `/asr-analyze session.cast` in Claude, markers appear in file

### Phase 4: Polish & Distribution - COMPLETE ✅
**Goal:** Production-ready tool with proper skill management

- [x] AGENTS.md, CLAUDE.md, GEMINI.md setup
- [x] Config file support (~/.config/asr/config.toml)
- [x] Uninstall script
- [x] README documentation
- [x] Test on Linux (via Docker build) - PR #7
- [x] Cross-compilation setup (.cargo/config.toml) - PR #7
- [x] Homebrew formula (Formula/asr.rb) - PR #7
- [x] **Skill management CLI:** - PR #8
  - `asr skills list` - Show installed skills and their locations
  - `asr skills install` - Extract embedded skills to agent command directories
  - `asr skills uninstall` - Remove skills from agent directories
  - **Skills embedded in binary** using `include_str!()` macro - no external files needed
  - Support: ~/.claude/commands/, ~/.codex/commands/, ~/.gemini/commands/
  - Updated install.sh to call `asr skills install` instead of symlinking

**Definition of Done:** ✅ Complete tool with `asr skills install/uninstall` commands

### Phase 5: Shell Integration & Automation
**Goal:** Seamless auto-recording with proper install/uninstall and auto-analysis

#### Shell Section Markers
The shell integration should use clear markers for easy updates/removal:
```bash
# >>> ASR (Agent Session Recorder) >>>
# DO NOT EDIT - managed by 'asr shell install/uninstall'
[ -f "/path/to/asr.sh" ] && source "/path/to/asr.sh"
# <<< ASR (Agent Session Recorder) <<<
```

#### Tasks
- [x] **Global auto-wrap toggle** - Config option `[shell] auto_wrap = true/false` - PR #10
- [x] **Shell management CLI:** - PR #10
  - `asr shell status` - Show if shell integration is active, which RC file
  - `asr shell install` - Add marked section to .zshrc/.bashrc
  - `asr shell uninstall` - Remove marked section cleanly
- [ ] **Auto-analyze hook** - Option to run `/asr-analyze` automatically after session ends
  - Config: `[recording] auto_analyze = true/false`
  - Calls the AI agent to analyze and add markers
- [x] **Update install.sh** - Use marked sections instead of simple append - PR #10
- [x] **Update uninstall.sh** - Properly remove marked sections from RC files - PR #10
- [ ] **Per-agent wrap control** - Optional: disable wrapping for specific agents

**Definition of Done:**
- `asr shell install` adds clean marked section
- `asr shell uninstall` removes it completely
- Global toggle to disable all wrapping without uninstalling
- Optional auto-analyze after each session

---

### Backlog (Future)

#### Enhanced Web Browser (extends asciinema-server)
- **Scrollable terminal view** - Browse session like a real terminal, scroll past sections
- **Auto-collapse long outputs** - Collapse verbose output (build logs, test results) into single expandable lines
- **Marker navigation** - Jump between marked points, filter by marker type
- **Session timeline** - Visual timeline showing markers, duration, key events

#### Other Ideas
- Session search/filtering by content
- Session tagging/categories
- Export/share functionality
- Usage analytics dashboard
