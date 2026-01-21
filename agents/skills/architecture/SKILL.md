---
name: architecture
description: Agent roles and orchestration patterns for multi-agent development. Defines coordinator, implementation, verification, and planning agent responsibilities.
---

# Agent Architecture

This skill defines the multi-agent orchestration pattern for AGR development.

## Foundation

**SDLC (Software Development Life Cycle)** is the underlying methodology used in this project. All agents operate within this framework:

```
Requirement -> Design -> Code -> Test -> Deploy -> Feedback -> (repeat)
```

Load `references/sdlc.md` first to understand the development cycle before assuming any role.

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

1. Load `sdlc.md` to understand the development framework
2. Load the reference matching the role assigned to you by the coordinator
3. Load `state.md` when working with `.state/` files
