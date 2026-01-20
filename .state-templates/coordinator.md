# Coordinator Session

## Role

You are a **COORDINATOR**. You do NOT implement code directly.

## Workflow

1. **Spawn Impl Agent** for task (using Task tool, `subagent_type=general-purpose`)
2. **Wait for PR** to be created
3. **Wait for CodeRabbit** review: `gh pr view <N> --comments`
4. **Spawn Verify Agent** (fresh session) to validate
5. **Merge** only after verification passes

## Current Status

| Field | Value |
|-------|-------|
| Phase | [PHASE_NAME] |
| Active Task | [TASK_DESCRIPTION] |
| PR | #[N] |
| Status | [PENDING/IN_PROGRESS/WAITING_REVIEW/READY_TO_MERGE] |

## Task Queue

| Task | Status | PR | Notes |
|------|--------|-----|-------|
| [Task 1] | PENDING | - | - |
| [Task 2] | PENDING | - | - |

## Agent Log

| Time | Type | Task | Result |
|------|------|------|--------|
| [TIMESTAMP] | Impl | [task] | PR #N created |
| [TIMESTAMP] | Verify | PR #N | PASS/FAIL |

## Commands

```bash
# Check PR status
gh pr checks <N>
gh pr view <N> --comments | grep -i coderabbit

# Merge after verification
gh pr merge <N> --squash --delete-branch
```

## Rules

- Never implement code directly
- Always spawn fresh agents (no context reuse)
- Wait for CodeRabbit before merging
- Update `.state/INDEX.md` at milestones
