# Requirements: Two-Phase Review Workflow

## Problem Statement
CodeRabbit triggers immediately when a PR is created, before any internal review has occurred. This wastes CodeRabbit's analysis on code that may have structural issues, ADR violations, or other problems that an internal Reviewer would catch. The result is:
- CodeRabbit feedback mixed with issues that should have been caught internally
- Potential for multiple CodeRabbit review cycles when one would suffice
- Inefficient use of external review resources

## Desired Outcome
A two-phase review process where:
1. **Phase 1 (Internal):** Code is reviewed internally before external tools see it
2. **Phase 2 (External):** CodeRabbit reviews only after internal review passes

This ensures CodeRabbit analyzes code that has already been vetted for structural correctness and ADR compliance.

## Scope

### In Scope
- Update `references/orchestrator.md` - modify flow diagram and step descriptions
- Update `references/reviewer.md` - clarify the Reviewer's role in internal vs post-CodeRabbit review
- Update `references/README.md` (roles skill) - update the SDLC flow diagram

### Out of Scope
- Changes to other roles (Architect, Implementer, Product Owner)
- Modifications to CodeRabbit configuration
- Changes to the requirements gathering or planning phases

## Acceptance Criteria
- [ ] Orchestrator flow shows Draft PR creation before internal review
- [ ] Orchestrator flow shows PR marked ready-for-review only after internal Reviewer approves
- [ ] Reviewer role documentation distinguishes between Phase 1 (internal) and Phase 2 (post-CodeRabbit) responsibilities
- [ ] README flow diagram reflects the two-phase approach
- [ ] The workflow clearly gates CodeRabbit behind internal review approval

## Proposed Flow (Reference for Implementation)
```
Current:
  Implementer → Create PR → CodeRabbit → Reviewer → Fix issues

Proposed:
  Implementer → Create Draft PR → Internal Reviewer → Fix internal issues →
  Mark PR Ready → CodeRabbit → Address CodeRabbit feedback → Done
```

## Constraints
- Draft PR mechanism must be compatible with GitHub's draft PR feature
- Internal Reviewer must have clear criteria for when to approve moving to Phase 2

## Context
- This change optimizes the existing SDLC workflow defined in the roles skill
- CodeRabbit is an external AI code review tool that runs on PR creation/updates
- The Reviewer role already exists but currently runs after CodeRabbit

---
**Sign-off:** Approved by user
