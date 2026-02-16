# Roles

Agent roles for orchestrated software development.

## Purpose

Roles separate concerns across the SDLC. Each role has a distinct responsibility and fresh context, preventing one agent from doing everything and accumulating bias.

## Flow

```
User Request
     │
     ▼
┌─────────────┐
│ Coordinator │  Coordinates, never implements
└──────┬──────┘
       │
       ▼
┌─────────────────┐
│  Product Owner  │  Requirements interview
│                 │  ◄── cross-consult: Architect (feasibility)
└────────┬────────┘
         │
         ▼
   ┌──────────────┐
   │REQUIREMENTS.md│  What needs to be built
   └───────┬──────┘
           │
           ▼
   ┌─────────────┐     ┌─────────┐
   │  Architect  │────▶│ ADR.md  │◀──────────────────────┐
   │             │     └─────────┘                       │
   │ ◄── cross-  │    Decision record (immutable)       │
   │  consult:PO │                                      │
   └──────┬──────┘                                      │
          │            ┌──────────┐                     │
          └───────────▶│ PLAN.md  │◀────────────┐       │
                       └────┬─────┘             │       │
                       Execution (mutable)      │       │
                            │                   │       │
                            ▼                   │       │
                  ┌─────────────────┐           │       │
                  │   Implementer   │  Works ───┘       │
                  └────────┬────────┘  from PLAN        │
                           │                            │
                     ┌─────┴──────┐                     │
                     │ Per-stage: │                     │
                     │ Pair Review│ questions/flags     │
                     └─────┬──────┘                     │
                           │                            │
                           ▼                            │
                     [Draft PR]                         │
                           │                            │
                           ▼                            │
                  ┌─────────────────┐                   │
                  │ Reviewer        │ Internal:         │
                  │ (adversarial)   │ full ADR+PLAN     │
                  └────────┬────────┘ check             │
                           │                            │
                      ┌────┴────┐                       │
                      │  Gate   │ Mark PR ready only    │
                      └────┬────┘ after internal pass   │
                           │                            │
                           ▼                            │
                    [CodeRabbit]  External review       │
                           │                            │
                           ▼                            │
                  ┌─────────────────┐                   │
                  │ Reviewer        │ Address CodeRabbit│
                  │ (coderabbit)    │ findings          │
                  └────────┬────────┘                   │
                           │                            │
                           ▼                            │
                  ┌─────────────────┐  Validates ───────┘
                  │  Product Owner  │  against REQUIREMENTS
                  └────────┬────────┘
                           │
                           ▼
                  ┌─────────────────┐
                  │   Maintainer    │  Merges, updates ADR Status
                  └─────────────────┘
```

## Agent-Based Architecture

Roles are implemented as agent files in `agents/agents/` (physically at `agents/agents/`). Each agent file contains:

1. **YAML frontmatter** - configuration (model, tools, permissions, skills, maxTurns)
2. **Markdown body** - role-specific behavioral instructions, workflow, and output format

The Coordinator spawns agents by name via `Task(agent-name, "prompt with context")`. Each spawned agent:
- Starts with its behavioral instructions from its agent file body
- Preloads shared protocols via the `skills: [roles, instructions]` field
- Has independent model/tool/permission configuration

The Coordinator agent body is the authoritative source for the SDLC flow. This README provides an overview.

## Design Documents

### ADR.md (Architecture Decision Record)

Immutable after approval. The contract everyone works against.

| Section | Purpose |
|---------|---------|
| Status | Proposed → Accepted → (Rejected/Superseded) |
| Context | Problem being solved, forces at play |
| Options | 2-3 approaches with pros/cons |
| Decision | Chosen option and why |
| Consequences | What becomes easier/harder, follow-ups |
| Decision History | Numbered log of decisions made with user |

Modified only by: Architect (with Product Owner approval in a formal loop)

### PLAN.md (Execution Plan)

Mutable during implementation. Detailed work tracking.

| Section | Purpose |
|---------|---------|
| Open Questions | Implementation challenges for implementer to solve |
| Stages | Tasks with goals, files, considerations |
| Dependencies | What must complete before what |
| Progress | Status tracking updated by implementer |

Modified by: Implementer (progress), Architect (scope changes via ADR loop)

## Roles

### Coordinator
- Coordinates the SDLC flow
- Never writes code
- Spawns other roles with fresh context
- Gates transitions between phases
- Orchestrates cross-consultation between PO and Architect
- Manages pair review lifecycle during implementation

### Product Owner
- Appears twice: requirements gathering (start) and validation (end)
- Conducts requirements interviews or drafts directly when clear
- Verifies ADR Context problem is solved
- Checks for scope creep
- Proposes splitting out-of-scope work

### Architect
- Creates ADR.md and PLAN.md
- Proposes 2-3 options, asks for user input
- Focuses on decisions, not implementation details
- Hands off only after explicit approval
- Can be consulted during PO phase for feasibility checks

### Implementer
- Works from PLAN.md stages
- Stays within ADR Decision scope
- Updates PLAN.md progress
- Creates PR when done
- Reports stage completion for pair review

### Reviewer (Three Phases)
- **Pair (reviewer-pair):** Collaborative incremental review during implementation. Asks questions, makes observations, flags potential issues per PLAN stage.
- **Internal (reviewer-internal):** Adversarial post-implementation review. Performs thorough code analysis, security review, and ADR compliance check before PR is marked ready.
- **CodeRabbit (reviewer-coderabbit):** Addresses external CodeRabbit findings. Implements fixes or documents clear rationale for dismissal.

### Maintainer
- Merges PR after approvals
- Updates ADR Status to Accepted
- Handles releases
- Monitors CI and manages PR lifecycle

## Key Principles

1. Fresh context - each role starts clean, no accumulated bias
2. ADR is the contract - implementation verified against it
3. PLAN is mutable - progress tracked without touching ADR
4. Scope discipline - out-of-scope work becomes new ADR cycle
5. Explicit approval - no phase transitions without sign-off
6. Agent files as configuration - each agent has its model, tools, and permissions defined in YAML frontmatter
7. Skills as shared protocols - cross-cutting concerns (collaboration protocol, verification rules, coding principles) loaded via the skills field

## Phases

A **phase** is a named operational mode of a role, represented by a separate agent file in `agents/agents/`. Each phase determines:
1. **Behavioral persona** (defined in agent file body)
2. **Agent configuration** (model, tools, permissions in frontmatter)
3. **Trigger context** (when in the SDLC the Coordinator spawns it)

A role without phases has a single agent file. A role with phases has one agent file per phase, named `<role>-<phase>.md`.

### Current Phase Definitions

- **reviewer-pair:** Collaborative review during implementation (per stage). Uses questions/observations/flags format. Spawned by Coordinator after each PLAN stage completion.
- **reviewer-internal:** Adversarial review after full implementation. Uses severity-classified findings. Spawned before PR is marked ready for external review.
- **reviewer-coderabbit:** Focused review for addressing external CodeRabbit findings. Implements fixes or documents dismissal rationale. Spawned after CodeRabbit completes its review.
