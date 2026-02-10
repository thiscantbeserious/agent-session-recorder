# State File Management

## State File Structure

```
.state/                      # Active state (minimal)
├── <branch>/                # Per-branch SDLC files (REQUIREMENTS/ADR/PLAN)
└── ...                      # Additional branch-scoped artifacts as needed

agents/skills/instructions/templates/  # Templates for state files
├── current-phase.md
├── phase-progress.md

.archive/                    # Historical archives
└── state/                   # Archived state snapshots
    └── YYYY-MM-DD-HHMMSS/   # Timestamped folders
```

## Before Starting Work

```bash
# 1. Check current state
gh pr list                       # Open PRs
gh pr list --state merged -L 10  # Recent completed work

# 2. Check recent context
gh pr list --state merged -L 10
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
# No global decisions file. Use branch ADRs in .state/<branch>/ instead.
```

## State Maintenance

### During Active Work

Use PR metadata as the live state source:
- PR title/body for current focus and scope
- PR comments/checks for blockers and review state
- Per-branch `.state/<branch>/ADR.md` and `.state/<branch>/PLAN.md` for design/execution state
- Merged PR history for completed work

### After Completing a Phase

**CRITICAL:** After merging PRs and completing a phase, the coordinator MUST:

1. **Clean up state files:**
   - Remove stale references to removed features

2. **Archive old state** if needed:
   ```bash
   TIMESTAMP=$(date +%Y-%m-%d-%H%M%S)
   mkdir -p .archive/state/$TIMESTAMP
   mv .state/old-file.md .archive/state/$TIMESTAMP/
   ```

3. **Commit state updates:**
   ```bash
   git add .state/
   git commit -m "chore(state): update after phase completion"
   git push
   ```

### State Hygiene Checklist (End of Phase)

- [ ] No stale references to removed features
- [ ] State changes committed and pushed

### Why This Matters

Stale state causes:
- New agents getting confused by outdated context
- Incorrect commands in examples
- Accumulated cruft that makes state files harder to read
