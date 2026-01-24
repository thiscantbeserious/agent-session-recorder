# ADR: SDLC Role System Improvements (Retrospective)

## Status
Accepted

## Context
This is a **retrospective ADR** documenting decisions made during the bootstrapping of the SDLC role system. The branch `chore/improve-roles-sdlc` contains 31 commits that established the foundational patterns for agent-based software development lifecycle management.

The initial state had a single combined document approach where decisions and execution tracking were mixed. This created several problems:
- Decisions could be accidentally modified during implementation
- Progress tracking cluttered the decision record
- Reviewers had difficulty distinguishing "what was decided" from "how it's being executed"
- No clear contract for validation

Forces at play:
- Agents need clear, immutable contracts to work against
- Implementation progress needs to be tracked without polluting decisions
- Fresh context for each role requires well-structured handoff documents
- Validation requires comparing implementation against stable criteria

## Options Considered

### Option 1: Single Combined Document
Keep everything in one file (decisions, options, tasks, progress).

- Pros: Simple, one file to track
- Cons: Mutable progress pollutes immutable decisions, hard to validate against

### Option 2: Separate ADR and PLAN Documents
Split into ADR.md (immutable decisions) and PLAN.md (mutable execution).

- Pros: Clear separation of concerns, ADR becomes stable contract, PLAN can evolve
- Cons: Two files to manage, need to keep them in sync

### Option 3: Database-Driven State
Store decisions and progress in structured data (JSON/YAML).

- Pros: Machine-readable, queryable
- Cons: Less human-readable, more tooling required, overkill for current needs

## Decision
**Option 2: Separate ADR and PLAN Documents**

The separation provides:
1. **Immutability** - ADR is frozen after approval, providing stable contract
2. **Flexibility** - PLAN can be updated by implementer without touching decisions
3. **Validation** - Reviewer and Product Owner validate against ADR, not shifting targets
4. **Simplicity** - Human-readable markdown, no additional tooling

Trade-offs accepted:
- Two files instead of one (acceptable overhead for clarity gained)
- Need to reference between documents (solved with explicit "References: ADR.md" in PLAN)

## Consequences

What becomes easier:
- Validation has stable criteria (ADR.md)
- Implementer can update progress without touching decisions
- Fresh-context roles can quickly understand "what was decided" vs "what's in progress"
- Scope creep is visible (changes to ADR require explicit approval loop)

What becomes harder:
- Initial setup requires creating both documents
- Must maintain consistency between ADR and PLAN

Follow-ups scoped for later:
- Tooling to auto-generate PLAN skeleton from ADR stages
- Validation scripts to check ADR/PLAN consistency
- Templates for common ADR patterns (bug fix, feature, refactor)

## Decision History

Decisions made during bootstrapping (reconstructed from git history):

1. Separated ADR.md (immutable) from PLAN.md (mutable) to prevent decision pollution during implementation (commit a8653a9)
2. Established `.state/<branch-name>/` convention for per-branch state files to keep state organized and branch-specific
3. Created templates in `agents/skills/roles/templates/` for ADR.md and PLAN.md to ensure consistent structure
4. Added "Decision History" section to ADR template for tracking design decisions made with user (commits 1a5b024, 4461a74)
5. Refined Orchestrator role with explicit gates: ADR approval before implementation, CodeRabbit review before Reviewer
6. Established restriction for agents to load only one role at a time to prevent context confusion
7. Added roles overview table to Orchestrator for quick reference without loading all role files
8. Created README.md documenting the flow diagram and design document purposes
9. Renamed decisions.md to PROJECT_DECISIONS.md for clarity about project-wide learnings vs per-branch ADRs
