# Agent Architecture

This document describes the multi-agent orchestration pattern used for developing AGR.

## IMPORTANT: You Are a Coordinator

**You MUST NOT implement code directly.** You are a COORDINATOR.

For any implementation task:
1. **Spawn Impl Agent** using Task tool (`subagent_type=general-purpose`)
2. **Wait for PR** to be created by the impl agent
3. **Wait for CodeRabbit** review: `gh pr view <N> --comments`
   - NEVER proceed while CodeRabbit shows "processing"
   - Must see actual review findings before continuing
4. **Spawn Verify Agent** (fresh session) to validate
5. **Only merge** after verification passes

```bash
# Check CodeRabbit status
gh pr view <PR_NUMBER> --comments | grep -i coderabbit

# If still processing, WAIT and check again
```

## Overview

AGR uses a **sequential orchestrator loop** with three distinct agent roles that communicate through state files. This pattern ensures quality through separation of concerns and fresh perspectives at each stage.

**Related docs:**
- `architecture/SDLC.md` - Agile SDLC phases (Requirement → Design → Code → Test → Deploy → Feedback)

## Agent Roles

### 1. Coordinator Agent

The coordinator orchestrates work but **never implements code directly**.

**Responsibilities:**
- Read state files to understand current context
- Plan work and break down tasks
- Spawn implementation agents for specific tasks
- Monitor progress via state files
- Gate PR merges after verification passes
- Document learnings in `decisions.md`

**State files used:**
- `.state/INDEX.md` - Entry point, where to find things
- `.state/decisions.md` - Technical decisions log
- GitHub PRs - Track completed/in-progress work

### 2. Implementation Agent

Spawned per-task to implement features on feature branches.

**Responsibilities:**
- Follow TDD: Red-Green-Refactor cycle
- Run `cargo test` and `./tests/e2e_test.sh`
- Create PR with clear description
- Create PR with progress
- Update `.state/INDEX.md` if needed

**Workflow:**
1. Claim task via lock file
2. Create feature branch
3. Implement with TDD
4. Run all tests
5. Create PR
6. Report completion

### 3. Verification Agent

Fresh session that validates implementation without context pollution.

**Responsibilities:**
- Run full test suite in clean environment
- Check CodeRabbit review (MANDATORY)
- Review PR diff and code quality
- Report findings to state files
- **Never merges** - just reports

**Why fresh sessions matter:**
- No context pollution from implementation
- Different perspective catches edge cases
- Clean validation environment

## Workflow Loop

```
┌─────────────────────────────────────┐
│     COORDINATOR SESSION             │
│  (Plans, spawns, monitors)          │
└──────────────┬──────────────────────┘
               │
               ▼
        ┌──────────────┐
        │ IMPL AGENT   │ ◄─ Spawned for task N
        │ feature/...  │
        └──────┬───────┘
               │ PR created
               ▼
        ┌──────────────┐
        │ CODERABBIT   │ ◄─ External review (auto-trigger)
        │ (GitHub App) │
        └──────┬───────┘
               │ Review posted
               ▼
        ┌──────────────┐
        │ VERIFY AGENT │ ◄─ Fresh session
        │ cargo test   │
        │ e2e tests    │
        │ CodeRabbit?  │
        └──────┬───────┘
               │
        ┌──────▼──────────┐
        │   PASS or FAIL? │
        └──────┬──────────┘
               │
       ┌───────┴────────┐
       ▼                ▼
    MERGE        ITERATE/FIX
       │           (feedback → new impl)
       │
       ▼
    Task N+1
```

**Sequential Rules:**
- Only ONE impl agent active at a time
- Wait for verification AND coordinator feedback before next task
- Each task builds on merged main + accumulated learnings
- No parallel task execution (avoids merge conflicts)

## State File Structure

```
.state/                      # Active state (minimal)
├── INDEX.md                 # Entry point - where to find things
├── decisions.md             # Technical decisions log
└── locks/                   # Task claiming mechanism
    └── .gitkeep

.state-templates/            # Templates (separate from active state)
├── INDEX.md
├── current-phase.md
├── phase-progress.md
└── decisions.md

.archive/                    # Historical archives
└── state/                   # Archived state snapshots
    └── YYYY-MM-DD-HHMMSS/   # Timestamped folders
```

### Tracking Progress via GitHub

Instead of duplicating state in markdown files, use GitHub:

```bash
# Completed work
gh pr list --state merged

# Current work
gh pr list

# Phase history
git branch -a | grep feature/
```

### Using Templates

When starting new work:

```bash
# Copy template as needed
cp .state-templates/decisions.md .state/decisions.md
```

## Communication Protocol

All agents communicate **asynchronously through state files**:

### Task Claiming (Lock Files)

Before working on a task:
```bash
# Check if task is claimed
if [ -f .state/locks/task-name.lock ]; then
  echo "Task claimed, pick another"
  exit 0
fi
# Claim it
echo "$(date +%s)" > .state/locks/task-name.lock
```

After completing:
```bash
rm .state/locks/task-name.lock
```

### Progress Tracking

Track progress via GitHub PRs:
- **Open PR** = In progress
- **Merged PR** = Completed
- **Draft PR** = Not ready for review

## Critical Rules

1. **Coordinator NEVER writes code** - only orchestrates
2. **CodeRabbit review is MANDATORY** - verify agent must wait for actual findings
3. **Sequential execution** - one task at a time, wait for full feedback loop
4. **Fresh agents per task** - prevents context pollution
5. **State files are single source of truth** - all communication via markdown
6. **E2E tests required** - `./tests/e2e_test.sh` must pass before any merge

## State Maintenance

### During Active Work

Keep `.state/INDEX.md` current as you work. Update state at these milestones:
- **Starting a task** - Update "Current focus"
- **Hitting a blocker** - Update "Blocked on"
- **Completing a subtask** - Note progress
- **Making a key decision** - Log in `decisions.md`
- **Creating a PR** - Update with PR number

This helps agents joining mid-session understand context and prevents lost progress if a session ends unexpectedly.

### After Completing a Phase

**CRITICAL:** After merging PRs and completing a phase, the coordinator MUST:

1. **Update `.state/INDEX.md`:**
   - Mark current focus as complete
   - Update "Recently completed" section
   - Clear any stale "Blocked on" entries

2. **Clean up state files:**
   - Remove old lock files from `.state/locks/`
   - Update `.state/decisions.md` with learnings from the phase
   - Remove stale references to removed features

3. **Archive old state** if needed:
   ```bash
   TIMESTAMP=$(date +%Y-%m-%d-%H%M%S)
   mkdir -p .archive/state/$TIMESTAMP
   mv .state/old-file.md .archive/state/$TIMESTAMP/
   ```

4. **Commit state updates:**
   ```bash
   git add .state/
   git commit -m "chore(state): update after phase completion"
   git push
   ```

### State Hygiene Checklist (End of Phase)

- [ ] `.state/INDEX.md` shows phase complete
- [ ] `.state/decisions.md` updated with new learnings
- [ ] Old lock files removed from `.state/locks/`
- [ ] No stale references to removed features
- [ ] State changes committed and pushed

### Why This Matters

Stale state causes:
- New agents getting confused by outdated context
- Incorrect commands in examples
- Accumulated cruft that makes state files harder to read

## Lessons Learned

Document in `.state/decisions.md`:

- **CodeRabbit Integration**: Never merge while CodeRabbit shows "processing" - must wait for actual review findings
- **Fresh Sessions Work**: Different perspective catches edge cases that impl agent misses
- **State Files Scale**: Async communication via files works for multi-agent coordination
- **Verify CodeRabbit Suggestions Locally**: NEVER blindly follow CodeRabbit suggestions. Always verify locally before implementing:
  - For CLI tool syntax issues, run `<tool> --help` to check actual command-line interface
  - For API suggestions, check the actual code or documentation
  - CodeRabbit may have outdated or incorrect information about third-party tools
  - Example: CodeRabbit suggested changing `codex exec prompt` to `codex --full-auto prompt`, but `codex --help` confirmed `exec` is the correct subcommand for non-interactive execution
