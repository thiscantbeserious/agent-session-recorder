# Coordinator Session State

## Current Status
- **Phase**: [PHASE_NUMBER] - [PHASE_NAME]
- **Mode**: Coordinator (does NOT implement directly)
- **Active Agents**: None

## Phase [N] Tasks

| Task | Status | Impl Agent | Verify Agent | PR |
|------|--------|------------|--------------|-----|
| [Task 1] | PENDING | - | - | - |
| [Task 2] | PENDING | - | - | - |

## Workflow

1. Spawn Impl Agent for task
2. Wait for completion (check .state/phase-N/impl-results/)
3. Spawn Verify Agent to validate
4. If PASS → merge PR
5. If FAIL → spawn new Impl Agent with fix instructions

## Agent Spawn Log

| Time | Type | Task | Agent ID | Status |
|------|------|------|----------|--------|
| YYYY-MM-DD | Impl | [task] | [id] | PENDING |

## Completed PRs

| PR | Task | Impl | Verify | Merged |
|----|------|------|--------|--------|

## Notes

- Always spawn fresh agents (no context reuse)
- Verify agents use different session than impl agents
- All communication through state files
- Never implement code in coordinator session
