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
│ Orchestrator│  Coordinates, never implements
└──────┬──────┘
       │
       ▼
┌─────────────┐     ┌─────────┐
│  Architect  │────▶│ ADR.md  │◀─────────────────────┐
└──────┬──────┘     └─────────┘                      │
       │            Decision record (immutable)      │
       │                                             │
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
                        ▼                            │
                   [Draft PR]                        │
                        │                            │
                        ▼                            │
               ┌─────────────────┐                   │
               │    Reviewer     │  Phase 1: Internal│
               │ (Phase: internal)  ADR+PLAN check  │
               └────────┬────────┘                   │
                        │                            │
                   ┌────┴────┐                       │
                   │  Gate   │ Mark PR ready         │
                   └────┬────┘                       │
                        │                            │
                        ▼                            │
                  [CodeRabbit]  External review      │
                        │                            │
                        ▼                            │
               ┌─────────────────┐                   │
               │    Reviewer     │  Phase 2: Address │
               │(Phase: coderabbit) findings        │
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

### Orchestrator
- Coordinates the SDLC flow
- Never writes code
- Spawns other roles with fresh context
- Gates transitions between phases

### Architect
- Creates ADR.md and PLAN.md
- Proposes 2-3 options, asks for user input
- Focuses on decisions, not implementation details
- Hands off only after explicit approval

### Implementer
- Works from PLAN.md stages
- Stays within ADR Decision scope
- Updates PLAN.md progress
- Creates PR when done

### Reviewer
- Validates implementation against ADR.md
- Runs tests, checks coverage
- Reports findings, never merges

### Product Owner
- Verifies ADR Context problem is solved
- Checks for scope creep
- Proposes splitting out-of-scope work
- Decides when to ship vs iterate

### Maintainer
- Merges PR after approvals
- Updates ADR Status to Accepted
- Handles releases

## Key Principles

1. Fresh context - each role starts clean, no accumulated bias
2. ADR is the contract - implementation verified against it
3. PLAN is mutable - progress tracked without touching ADR
4. Scope discipline - out-of-scope work becomes new ADR cycle
5. Explicit approval - no phase transitions without sign-off
