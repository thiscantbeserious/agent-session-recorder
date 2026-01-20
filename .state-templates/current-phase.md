# Current State

## Session Mode: [COORDINATOR|IMPL|VERIFY]

This session is a **[role]** that [description].

**Key State Files:**
- `.state/plan.md` - Full project plan (version controlled)
- `.state/coordinator.md` - Active agent tracking
- `.state/decisions.md` - Technical decisions log

## Quick Start for New Session
```bash
cd [PROJECT_PATH]
cargo test && ./tests/e2e_test.sh       # Verify everything works
```

## Project Info
- **Location:** [PROJECT_PATH]
- **Repo:** [REPO_URL]
- **License:** MIT

## Current Phase: [N] - [PHASE_NAME]

### Status: [IN_PROGRESS|COMPLETE]

### Tasks
- [ ] Task 1
- [ ] Task 2
- [ ] Task 3

### What's Been Done
[Summary of completed work]

### What's Next
[Next steps]

## Git Workflow
```bash
# Always create feature branch from main
git checkout main && git pull
git checkout -b feature/[phase]-[task-name]

# Work, commit frequently
cargo test && ./tests/e2e_test.sh  # MUST pass
git add -A && git commit -m "feat(scope): description"

# Push and create PR
git push -u origin feature/[phase]-[task-name]
gh pr create --base main --title "feat: ..."

# After approval, merge
gh pr merge N --squash
```

## Important Notes
1. E2E tests: MANDATORY before any merge
2. Read .state files before starting work
3. Update decisions.md with technical decisions
