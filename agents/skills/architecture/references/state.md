# State File Management

## State File Structure

```
.state/                      # Active state (minimal)
├── INDEX.md                 # Entry point - where to find things
├── decisions.md             # Technical decisions log
└── locks/                   # Task claiming mechanism
    └── .gitkeep

agents/skills/architecture/templates/  # Templates for state files
├── INDEX.md
├── current-phase.md
├── phase-progress.md
└── decisions.md

.archive/                    # Historical archives
└── state/                   # Archived state snapshots
    └── YYYY-MM-DD-HHMMSS/   # Timestamped folders
```

## Before Starting Work

```bash
# 1. Check current state
cat .state/INDEX.md
gh pr list                       # Open PRs
gh pr list --state merged -L 10  # Recent completed work

# 2. Check decisions
cat .state/decisions.md
```

## Task Locking (for parallel subagents)

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

## Tracking Progress via GitHub

Instead of duplicating state in markdown files, use GitHub:

```bash
# Completed work
gh pr list --state merged

# Current work
gh pr list

# Phase history
git branch -a | grep feature/
```

## Using Templates

When starting new work:

```bash
# Copy template as needed
cp agents/skills/architecture/templates/decisions.md .state/decisions.md
```

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
