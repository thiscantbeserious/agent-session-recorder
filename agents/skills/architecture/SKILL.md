---
name: architecture
description: Agent roles and orchestration patterns. You MUST READ the role file for your assigned role before doing any work.
---

# Agent Architecture

**IMPORTANT:** You MUST use your Read tool to actually read the role file. Don't just acknowledge it exists.

## Your Role

1. **READ** `references/orchestrator.md` if you are the coordinator (default role)
2. **READ** the specific role file if assigned a different role by the coordinator

## SDLC Framework

All agents operate within this cycle:
```
Requirement -> Design -> Code -> Test -> Deploy -> Feedback -> (repeat)
```

READ `references/sdlc.md` to understand this framework.

## Agent Roles

| Reference | Contains |
|-----------|----------|
| Orchestrator | Coordinator responsibilities, workflow loop, spawning agents |
| Implementation | Feature branch workflow, TDD cycle, PR creation |
| Verification | Fresh-perspective validation, test suite execution, review |
| Planning | Design phase, approach evaluation, plan documentation |
| State | State file management, locks, templates |

## Templates

State file templates are in `templates/` folder:
- `INDEX.md` - Entry point template
- `current-phase.md` - Phase tracking template
- `phase-progress.md` - Progress template
- `decisions.md` - Decisions log template

## Usage

1. **READ** `references/sdlc.md` to understand the development framework
2. **READ** the reference file matching your assigned role
3. **READ** `references/state.md` when working with `.state/` files
