# PLAN: Two-Phase Review Workflow

## Open Questions

None - approach is well-defined.

## Stages

### Stage 1: Update Orchestrator Flow Diagram

**Goal:** Modify the flow diagram in orchestrator.md to show Draft PR and two review phases.

**Files:**
- `.claude/skills/roles/references/orchestrator.md`

**Changes:**
- [x] Update ASCII flow diagram to show:
  - Implementer creates Draft PR (not regular PR)
  - Internal Reviewer validates before marking ready
  - PR marked ready-for-review triggers CodeRabbit
  - Second Reviewer phase for CodeRabbit feedback
- [x] Show the gate between internal review and CodeRabbit clearly

**Considerations:**
- Keep diagram readable despite added complexity
- Use consistent box/arrow styling

---

### Stage 2: Update Orchestrator Steps

**Goal:** Modify the numbered steps section to reflect two-phase review.

**Files:**
- `.claude/skills/roles/references/orchestrator.md`

**Changes:**
- [x] Step 3: Change "PR" to "Draft PR"
- [x] Add Step 4: Spawn Reviewer with `Phase: internal`
- [x] Add Step 5: Gate - only proceed if internal review passes, then mark PR ready
- [x] Update Step 6: Wait for CodeRabbit (now after PR marked ready)
- [x] Update Step 7: Spawn Reviewer with `Phase: coderabbit`
- [x] Renumber subsequent steps (Product Owner validation, Maintainer merge)

**Considerations:**
- Maintain clear gate language
- Update step numbers consistently

---

### Stage 3: Update Orchestrator Spawning Template

**Goal:** Add phase parameter to the Reviewer spawning template example.

**Files:**
- `.claude/skills/roles/references/orchestrator.md`

**Changes:**
- [x] Update "Spawning Roles" section example to show phase parameter:
  ```
  You are the Reviewer.

  <paste full content from references/reviewer.md here>

  Phase: internal  # or "coderabbit"
  Branch: <branch-name>
  ...
  ```

**Considerations:**
- Keep example clear and copy-pastable
- Show both phase values as comment

---

### Stage 4: Update Reviewer Role Documentation

**Goal:** Add Phase 1 and Phase 2 sections with distinct responsibilities.

**Files:**
- `.claude/skills/roles/references/reviewer.md`

**Changes:**
- [x] Add "## Phase Parameter" section explaining the two phases
- [x] Add "## Phase 1: Internal Review" section with:
  - Focus: ADR compliance, PLAN completion, tests, scope
  - Process: Review before PR is marked ready
  - Output: Approve to proceed or request changes
- [x] Add "## Phase 2: CodeRabbit Review" section with:
  - Focus: Address CodeRabbit findings
  - Process: Review after CodeRabbit completes
  - Output: Approve to merge or request changes
- [x] Update "Review Process" section to reference phases
- [x] Keep shared checklist and reporting format

**Considerations:**
- Maintain clarity about which phase applies when
- Keep common elements (test running, reporting format) unified

---

### Stage 5: Update README Flow Diagram

**Goal:** Update the roles README.md flow diagram to reflect two-phase review.

**Files:**
- `.claude/skills/roles/README.md`

**Changes:**
- [x] Update ASCII flow diagram to show:
  - Draft PR creation
  - Internal Reviewer phase
  - PR marked ready
  - CodeRabbit phase
  - Reviewer (CodeRabbit) phase
- [x] Keep diagram aligned with orchestrator.md diagram

**Considerations:**
- README diagram is simpler than orchestrator - may need less detail
- Ensure consistency between both diagrams

---

### Stage 6: Verification

**Goal:** Verify all changes are consistent and acceptance criteria met.

**Files:**
- All modified files

**Checks:**
- [x] Orchestrator flow shows Draft PR before internal review
- [x] Orchestrator flow shows PR marked ready only after internal approval
- [x] Reviewer documentation distinguishes Phase 1 vs Phase 2
- [x] README flow reflects two-phase approach
- [x] CodeRabbit clearly gated behind internal review
- [x] All diagrams consistent with each other

## Dependencies

```
Stage 1 ──┐
          ├──▶ Stage 4 ──▶ Stage 5 ──▶ Stage 6
Stage 2 ──┤
          │
Stage 3 ──┘
```

- Stages 1, 2, 3 can be done in parallel (all orchestrator.md changes)
- Stage 4 (reviewer.md) can start after Stage 3 (needs to match spawning template)
- Stage 5 (README) should follow Stage 4 (needs to match reviewer phases)
- Stage 6 (verification) is final

## Progress

| Stage | Status | Notes |
|-------|--------|-------|
| 1. Orchestrator Flow Diagram | Complete | Added Draft PR, two Reviewer phases, Gate box |
| 2. Orchestrator Steps | Complete | Steps 1-9 with Draft PR, Phase 1/2 reviews, Gate |
| 3. Orchestrator Spawning Template | Complete | Added "Spawning the Reviewer" subsection with Phase parameter |
| 4. Reviewer Documentation | Complete | Added Phase Parameter, Phase 1, Phase 2 sections |
| 5. README Flow Diagram | Complete | Updated to match orchestrator diagram |
| 6. Verification | Complete | All acceptance criteria verified |
