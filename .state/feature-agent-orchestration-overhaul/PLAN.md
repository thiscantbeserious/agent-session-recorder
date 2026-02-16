# Plan: Agent Orchestration Overhaul

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. Does `cargo xtask validate-workflow` need to validate agent file frontmatter (YAML parsing), or just check file existence and banned terms? The current validator checks for banned strings and required content in role references. With references deleted, it must shift to validating agent files.
2. Should the `roles` skill README.md flow diagram be updated to match the new Coordinator flow diagram, or should the README defer to the Coordinator agent body as the authoritative flow source?

## Stages

### Stage 1: Create Agent Directory and Agent Files

Goal: Create the `.claude/agents/` directory (physically `agents/agents/`) and all 8 agent files with complete frontmatter and body content as specified in the ADR.

Owner: implementer

- [ ] Create `agents/agents/` directory
- [ ] Create `agents/agents/coordinator.md` with full frontmatter (model haiku, tools with Task declarations, disallowedTools, permissionMode, skills) and complete body content (starting a cycle, spawning agents, cross-consultation protocol, pair review lifecycle, boundaries, role-to-role routing, parallel mode, flow diagram, steps, transition gates, deterministic checks)
- [ ] Create `agents/agents/product-owner.md` with frontmatter (Read/Grep/Glob/Write tools, disallowed Edit/Bash/NotebookEdit/WebFetch/WebSearch, maxTurns 30) and body (requirements gathering, interview structure, cross-consultation guidance, validation, sign-off)
- [ ] Create `agents/agents/architect.md` with frontmatter (model opus, Read/Grep/Glob/Write/Bash tools, disallowed Edit/NotebookEdit/WebFetch/WebSearch, maxTurns 40) and body (mindset, design process, cross-consultation guidance, input/output, key rules)
- [ ] Create `agents/agents/implementer.md` with frontmatter (Read/Write/Edit/Grep/Glob/Bash/NotebookEdit tools, disallowed WebFetch/WebSearch, acceptEdits, maxTurns 75) and body (required reading, workflow, stage completion reporting, TDD cycle, verification)
- [ ] Create `agents/agents/reviewer-pair.md` with frontmatter (model haiku, Read/Grep/Glob/Bash tools, disallowed Edit/Write/NotebookEdit/WebFetch/WebSearch, maxTurns 15) and body (mindset, review scope, output format with questions/observations/flags, limitations)
- [ ] Create `agents/agents/reviewer-internal.md` with frontmatter (model opus, Read/Grep/Glob/Bash tools, disallowed Edit/Write/NotebookEdit/WebFetch/WebSearch, maxTurns 40) and body (mindset, pair review context handling, severity classification, review steps 1-8, output format, anti-patterns, key rules)
- [ ] Create `agents/agents/reviewer-coderabbit.md` with frontmatter (Read/Write/Edit/Grep/Glob/Bash tools, disallowed NotebookEdit/WebFetch/WebSearch, acceptEdits, maxTurns 40) and body (workflow for addressing CodeRabbit findings, key rules)
- [ ] Create `agents/agents/maintainer.md` with frontmatter (Read/Grep/Glob/Bash/Edit tools, disallowed Write/NotebookEdit/WebFetch/WebSearch, maxTurns 25) and body (PR workflow, release process, key rules)
- [ ] Verify all 8 files parse valid YAML frontmatter (manual check or script)

Files: `agents/agents/coordinator.md`, `agents/agents/product-owner.md`, `agents/agents/architect.md`, `agents/agents/implementer.md`, `agents/agents/reviewer-pair.md`, `agents/agents/reviewer-internal.md`, `agents/agents/reviewer-coderabbit.md`, `agents/agents/maintainer.md`
Depends on: none

Considerations:
- The ADR section "2. Complete Agent File Content" contains the exact content for all 8 files. Use it as the source of truth.
- The physical path is `agents/agents/` because `.claude` is a symlink to `agents/`. Verify the symlink resolves correctly after creation.
- YAML frontmatter `tools` field for the Coordinator uses the `Task(agent1, agent2, ...)` syntax for spawning restrictions.
- Edge case: ensure the `skills` field uses the correct skill names (`roles` and `instructions`) matching the SKILL.md `name` frontmatter fields.

### Stage 2: Update SKILL.md with New Sections

Goal: Update `.claude/skills/roles/SKILL.md` to add cross-consultation protocol (Section 5), phase concept (Section 6), and update Section 1 access pattern to reflect agent-based spawning instead of reference-file loading.

Owner: implementer

- [ ] Update Section 1 (Access pattern): change "default to `references/coordinator.md`" to "default to the coordinator agent". Update the role-assignment paragraph to reference agent file bodies instead of `references/` loading. Keep startup policy, Direct Assist policy, quick implementation loop, and deterministic checks unchanged.
- [ ] Keep Section 2 (Restriction) unchanged
- [ ] Keep Section 3 (Role-to-Role Collaboration Protocol) unchanged
- [ ] Keep Section 4 (Verification) unchanged
- [ ] Add Section 5 (Cross-Consultation Protocol): triggers (lead role request, Coordinator judgment, user request), guard rails (max 3 per phase, max 2 follow-ups, lead role owns artifact, escalation to user), allowed consultations table (PO phase -> Architect, Architect phase -> PO)
- [ ] Add Section 6 (Phases): definition of phases, naming convention (`<role>-<phase>.md`), current phase definitions (reviewer-pair, reviewer-internal, reviewer-coderabbit)
- [ ] Verify that existing required content strings still pass `validate-workflow`: "Direct Assist", "Role-to-Role Collaboration Protocol", "Without `/roles`, never start this loop without explicit user confirmation."

Files: `agents/skills/roles/SKILL.md`
Depends on: none

Considerations:
- The `validate-workflow` xtask checks for specific strings in SKILL.md. All three required strings must be preserved exactly.
- Do NOT change the collaboration protocol format (Section 3) -- it is the shared protocol that all agents use.
- The new sections (5, 6) extend the existing content, they don't replace it.

### Stage 3: Update Roles README

Goal: Update `.claude/skills/roles/README.md` to reflect the new agent-based architecture, updated flow diagram with pair review and cross-consultation, and the phase concept.

Owner: implementer

- [ ] Update the Flow diagram to include cross-consultation arrows (PO <-> Architect) and pair review step between implementation stages
- [ ] Update the Roles section to mention the three reviewer phases (pair, internal, coderabbit) instead of a single Reviewer
- [ ] Update the "Key Principles" section to add: agent files as configuration, skills as shared protocols
- [ ] Add a section on Phases explaining the `<role>-<phase>.md` naming convention
- [ ] Update any references to "references/" directory to explain the new agent-body-based architecture

Files: `agents/skills/roles/README.md`
Depends on: Stage 1, Stage 2

Considerations:
- The README is informational documentation. It should reflect the architecture but does not affect runtime behavior.
- Keep it concise -- the Coordinator agent body is the authoritative source for the flow. The README provides an overview.

### Stage 4: Delete Reference Files

Goal: Remove the 6 role reference files whose content has migrated to agent file bodies.

Owner: implementer

- [ ] Delete `agents/skills/roles/references/coordinator.md`
- [ ] Delete `agents/skills/roles/references/product-owner.md`
- [ ] Delete `agents/skills/roles/references/architect.md`
- [ ] Delete `agents/skills/roles/references/implementer.md`
- [ ] Delete `agents/skills/roles/references/reviewer.md`
- [ ] Delete `agents/skills/roles/references/maintainer.md`
- [ ] Verify that `agents/skills/roles/references/` directory is empty (or remove it if no templates remain)
- [ ] Grep entire codebase for remaining references to `references/coordinator.md`, `references/reviewer.md`, etc. and update any stale paths

Files: `agents/skills/roles/references/coordinator.md`, `agents/skills/roles/references/product-owner.md`, `agents/skills/roles/references/architect.md`, `agents/skills/roles/references/implementer.md`, `agents/skills/roles/references/reviewer.md`, `agents/skills/roles/references/maintainer.md`
Depends on: Stage 1, Stage 2

Considerations:
- The `templates/` subdirectory (ADR.md, PLAN.md, REQUIREMENTS.md, REVIEW.md) is NOT deleted. Templates remain in `agents/skills/roles/templates/`.
- Watch for references in AGENTS.md, CLAUDE.md, README.md, or any instruction files that point to the old reference paths.
- The validate-workflow xtask will FAIL after this stage until Stage 5 updates it. Stage 5 must be completed before running validation.

### Stage 5: Update validate-workflow Xtask

Goal: Update `xtask/src/workflow/validate.rs` to validate the new agent files instead of the deleted reference files. Ensure all existing invariants are preserved or translated to the new file structure.

Owner: implementer

- [ ] Update the `files` array to replace reference file paths with agent file paths: `agents/agents/coordinator.md`, `agents/agents/product-owner.md`, `agents/agents/architect.md`, `agents/agents/implementer.md`, `agents/agents/reviewer-pair.md`, `agents/agents/reviewer-internal.md`, `agents/agents/reviewer-coderabbit.md`, `agents/agents/maintainer.md`
- [ ] Keep `agents/skills/roles/SKILL.md` and `agents/skills/instructions/references/state.md` in the files array
- [ ] Update the banned-term check: same banned terms apply to agent files
- [ ] Update the `orchestrator` legacy term check: apply to agent files in `agents/agents/` (path prefix changes from `agents/skills/roles/` to `agents/agents/`)
- [ ] Update the Coordinator-specific checks: "Relay and gate only" and "must not make domain, requirements, or technical solution decisions" and "always requires explicit user confirmation before spawning Implementer/Reviewer" must now be found in `agents/agents/coordinator.md` instead of `agents/skills/roles/references/coordinator.md`
- [ ] Keep SKILL.md checks unchanged (Direct Assist, collaboration protocol, confirmation rule)
- [ ] Keep state.md and PLAN template checks unchanged
- [ ] Add new validation: check that all 8 agent files exist in `agents/agents/`
- [ ] Add new validation: check that `agents/agents/coordinator.md` contains `Task(` (spawning declaration)
- [ ] Run `cargo xtask validate-workflow` and confirm it passes
- [ ] Run `cargo test` for the xtask crate

Files: `xtask/src/workflow/validate.rs`
Depends on: Stage 1, Stage 2, Stage 4

Considerations:
- The validator reads files relative to the working directory. Agent files are at `agents/agents/` (physical path), not `.claude/agents/`.
- The `reviewer.md` single file is replaced by three files. The old path `agents/skills/roles/references/reviewer.md` is removed; three new paths are added.
- The banned-term list and orchestrator-term check must work with the new paths.
- Edge case: the old code checks `file.starts_with("agents/skills/roles/")` for the orchestrator term check. This needs updating to also cover `agents/agents/`.

### Stage 6: Update AGENTS.md and Cross-References

Goal: Update the top-level AGENTS.md (which is also CLAUDE.md via symlink behavior) and any other files that reference the old role-loading pattern or reference file paths.

Owner: implementer

- [ ] Update AGENTS.md Section 1 to reference agent-based spawning instead of skill-based role loading. The "Auto-load the `roles` skill" instruction remains (skills still provide shared protocols), but add context that agents are now spawned by name.
- [ ] Grep for any remaining references to `references/coordinator.md`, `references/reviewer.md`, `references/architect.md`, `references/implementer.md`, `references/product-owner.md`, `references/maintainer.md` across the entire codebase and update them
- [ ] Check `agents/skills/instructions/references/sdlc.md` for any references to the old role-loading pattern and update if needed
- [ ] Check `agents/skills/instructions/references/state.md` for any references that need updating
- [ ] Run `cargo xtask validate-workflow` as final verification

Files: `AGENTS.md`, `agents/skills/instructions/references/sdlc.md`
Depends on: Stage 5

Considerations:
- AGENTS.md is the entry point that Claude Code loads. It must correctly guide the main thread to use the roles skill and understand the agent-based spawning model.
- The `instructions` skill references (sdlc.md, state.md, etc.) may contain references to the old role file paths. These need to be found and updated.
- This is the final stage -- after this, `cargo xtask validate-workflow` should pass and all cross-references should be consistent.

## Dependencies

What must be done before what:

- Stage 1 and Stage 2 are independent and can be worked in parallel
- Stage 3 depends on Stage 1 (agent files exist) and Stage 2 (SKILL.md updated) to ensure the README reflects the final state
- Stage 4 depends on Stage 1 (agent files exist as the replacement) and Stage 2 (SKILL.md no longer references them)
- Stage 5 depends on Stage 1 (agent files to validate), Stage 2 (updated SKILL.md), and Stage 4 (reference files deleted, so validator must point to new files)
- Stage 6 depends on Stage 5 (validator works) for final cross-reference cleanup

```
Stage 1 ──┬──► Stage 3 ──► (informational, no downstream dependency)
           │
           ├──► Stage 4 ──► Stage 5 ──► Stage 6
           │         ▲
Stage 2 ──┘─────────┘
```

Parallel opportunities:
- Stage 1 and Stage 2 can run in parallel (no file overlap)
- Stage 3 can run in parallel with Stage 4 (no file overlap, but Stage 3 needs Stage 1+2 complete)

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | pending | 8 agent files |
| 2 | pending | SKILL.md update |
| 3 | pending | README update |
| 4 | pending | Delete 6 reference files |
| 5 | pending | Update validate-workflow |
| 6 | pending | Cross-reference cleanup |
