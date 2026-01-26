# ADR: Two-Phase Review Workflow

## Status

Accepted

## Context

CodeRabbit, our external AI code review tool, triggers immediately when a PR is created. This creates an inefficient workflow where CodeRabbit analyzes code before any internal review has occurred. The problems include:

1. **Wasted analysis cycles** - CodeRabbit feedback gets mixed with issues that internal review would catch (structural problems, ADR violations, scope creep)
2. **Multiple review iterations** - Code that fails internal review requires additional CodeRabbit cycles after fixes
3. **Resource inefficiency** - External review resources spent on code that isn't ready

The current flow is:
```
Implementer -> Create PR -> CodeRabbit triggers -> Reviewer -> Fix issues -> More CodeRabbit cycles
```

The desired flow gates CodeRabbit behind internal review:
```
Implementer -> Draft PR -> Internal Review -> Fix issues -> Mark Ready -> CodeRabbit -> Address feedback -> Done
```

This ensures CodeRabbit only sees code that has passed internal validation for ADR compliance, test coverage, and scope adherence.

## Options Considered

### Option 1: Two Separate Reviewer Roles

Create distinct `InternalReviewer` and `ExternalReviewer` roles with separate reference files.

**Pros:**
- Maximum clarity - each role has single responsibility
- No conditional logic needed
- Role selection is explicit

**Cons:**
- File duplication - much of the review process is identical
- Maintenance burden - updates needed in two places
- Complexity - more roles to coordinate

### Option 2: Unified Reviewer with Internal Sections

Keep single `reviewer.md` but add internal Phase 1 and Phase 2 sections that the Reviewer interprets contextually.

**Pros:**
- Single file to maintain
- All review knowledge in one place

**Cons:**
- Ambiguity - Reviewer must determine which phase applies
- No explicit signal from Orchestrator about current phase
- Risk of confusion when spawning the role

### Option 3: Single Reviewer Role with Phase Parameter

Keep single `reviewer.md` with distinct Phase 1 and Phase 2 sections. Orchestrator explicitly passes the phase when spawning:

```
You are the Reviewer.

<paste reviewer.md content>

Phase: internal  # or "coderabbit"
Branch: <branch-name>
...
```

**Pros:**
- Single file to maintain
- Explicit phase signal from Orchestrator
- Clear separation of responsibilities per phase
- Orchestrator controls the workflow gate

**Cons:**
- Slightly more complex spawning template
- Reviewer must parse phase parameter

## Decision

**Option 3: Single Reviewer Role with Phase Parameter**

The Orchestrator will spawn the Reviewer twice during the SDLC cycle:
1. **Phase 1 (internal)** - After Implementer creates Draft PR, before marking ready
2. **Phase 2 (coderabbit)** - After PR is marked ready and CodeRabbit completes

This approach:
- Keeps review knowledge centralized in one file
- Gives Orchestrator explicit control over the gate
- Maintains the principle of fresh context per spawn
- Clearly separates internal validation from external feedback processing

### Phase Responsibilities

**Phase 1 (Internal Review):**
- Validate implementation against ADR Decision
- Check all PLAN.md stages completed
- Run tests and check coverage
- Verify scope adherence
- Report: approve to proceed or request changes

**Phase 2 (CodeRabbit Review):**
- Review CodeRabbit findings
- Address or dismiss each finding with rationale
- Verify no regressions from fixes
- Report: approve to merge or request changes

## Consequences

### What Becomes Easier
- CodeRabbit only analyzes vetted code
- Single review cycle for external tools (typically)
- Clear gate preventing premature external review
- Centralized review documentation

### What Becomes Harder
- Two Reviewer spawns per cycle instead of one
- Draft PR workflow required (already GitHub native)
- Slightly longer SDLC timeline per feature

### Follow-up Work
- None identified - changes are isolated to role documentation

## Decision History

1. **2024-01-26** - Three options presented to user: separate roles, unified with sections, single role with phase parameter
2. **2024-01-26** - User approved Option 3: Single Reviewer Role with Phase Parameter
