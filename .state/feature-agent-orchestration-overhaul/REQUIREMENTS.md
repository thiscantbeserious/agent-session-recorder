# Requirements: Agent Orchestration Overhaul

## Problem Statement

The current SDLC agent workflow is rigidly sequential: Product Owner finishes requirements completely, then hands off to the Architect, who finishes design completely, then hands off to the Implementer, and so on. Each role is a stateless subagent that starts fresh, communicates only through the Coordinator, and has no configurable properties beyond its markdown system prompt. The Coordinator manually injects role prompts into Task tool calls rather than using Claude Code's native agent mechanism.

This creates three problems:

1. **No model control and no native agent configuration.** All roles use whatever model Claude Code defaults to. The Coordinator manually pastes role markdown into Task tool prompts instead of using `.claude/agents/` -- Claude Code's native mechanism for defining agents with per-agent model selection, tool restrictions, permission modes, skills injection, and max turn limits. This wastes the platform's built-in capabilities and makes roles harder to configure and maintain.

2. **Sequential bottleneck in early phases.** The PO and Architect work in strict sequence, but requirements and design naturally inform each other. The user must fully finalize requirements before any design thinking begins, even though early Architect input could shape better requirements, and ongoing PO involvement could keep the Architect aligned during design.

3. **Review happens too late.** The Reviewer only appears after the entire implementation is complete. Issues that could have been caught during implementation (wrong direction, misunderstood requirements, questionable patterns) are only discovered after all the code is written, leading to costly rework.

## Architecture

This feature introduces a clean two-layer separation of concerns:

### Configuration Layer: `.claude/agents/`

Each spawnable role gets a `.md` file in `.claude/agents/` with YAML frontmatter defining its operational configuration:
- `model` -- which Claude model to use (sonnet, opus, haiku)
- `tools` -- which tools the agent can access (including which other agents it can spawn)
- `disallowedTools` -- tools explicitly denied
- `permissionMode` -- permission handling
- `maxTurns` -- turn limit for the agent
- `skills` -- skills to inject (loads skill content into agent context)
- `mcpServers`, `hooks`, `memory` -- additional Claude Code native config

The Coordinator spawns agents by name via the Task tool (e.g., `Task(implementer, "implement stage 1")`) instead of manually injecting role prompts.

### Behavioral Layer: `.claude/skills/roles/references/`

Role behavioral instructions (personas, workflows, output formats, collaboration protocols) remain in skill files. These are injected into agents via the `skills` field in the agent frontmatter, maintaining the existing separation where skills define behavior and agents define configuration.

### Phase Variants as Separate Agent Files

Since each agent file has exactly one `model` setting, role phases that require different models need separate agent files. The Reviewer role has three phases with distinct personas and potentially different models:

| Agent File | Phase | Model (example) | Behavioral Skill |
|---|---|---|---|
| `reviewer-pair.md` | pair | haiku | Pair review persona from skills |
| `reviewer-internal.md` | internal | opus | Adversarial review persona from skills |
| `reviewer-coderabbit.md` | coderabbit | sonnet | CodeRabbit response persona from skills |

The Coordinator spawns the appropriate reviewer agent by name based on the workflow phase. Roles without phases have a single agent file (e.g., `architect.md`, `implementer.md`).

### What This Replaces

- **Before:** Coordinator reads role markdown from `.claude/skills/roles/references/`, manually pastes it into Task tool prompts, has no per-agent model/tool control
- **After:** Coordinator spawns named agents from `.claude/agents/`, each preconfigured with model, tools, skills, and permissions via native Claude Code mechanism

## Desired Outcome

After implementation, the SDLC workflow should support:

- **Native agent configuration.** Each spawnable role is a `.claude/agents/*.md` file with YAML frontmatter controlling model, tools, permissions, and skills. Phase variants (e.g., the three reviewer phases) are separate agent files with independent configuration. The Coordinator spawns agents by name rather than injecting prompts manually.

- **Bidirectional cross-consultation between roles.** During the PO-led requirements phase, the Architect is available for feasibility checks. During the Architect-led design phase, the PO is available for alignment checks. Each phase retains a clear lead role and a clear artifact, but the Coordinator can pull in the secondary role for targeted consultations. The user remains an active participant throughout.

- **Three-phase review lifecycle.** A lightweight, collaborative "pair" reviewer participates during implementation (asking questions, flagging concerns early). The existing adversarial "internal" reviewer performs thorough post-implementation review. The existing "coderabbit" reviewer addresses external findings. Each phase is a separate agent file with its own model and configuration.

## Scope

### In Scope

- **`.claude/agents/` agent files for all spawnable roles**: Create agent files with YAML frontmatter (model, tools, skills, permissions, maxTurns) for: Product Owner, Architect, Implementer, Reviewer (3 phase variants), and Maintainer. Total: 8 agent files (5 single-phase roles + 3 reviewer variants).
- **Phase concept as a first-class construct**: A strict definition of what "phases" are -- named operational modes of a role, each represented by a separate agent file with independent configuration. The Reviewer is the first role with phases (pair, internal, coderabbit). The pattern is general enough for other roles to gain phases in the future by adding agent files.
- **Tool restrictions per agent**: Each agent file defines which tools it can access. For example, the Implementer gets file editing and git tools; the Reviewer gets read-only tools; the PO gets no code tools. The Coordinator's `tools` field lists which agents it can spawn.
- **Skills injection**: Agent files use the `skills` field to load behavioral instructions from `.claude/skills/roles/references/`, maintaining the config/behavior separation.
- **Bidirectional cross-consultation**: Updates to the Coordinator's orchestration rules to allow spawning a secondary agent as a short-lived consultant during another role's phase. PO can consult Architect during requirements; Architect can consult PO during design. Uses the existing Role-to-Role Collaboration Protocol (structured request/response format from SKILL.md Section 3) with additional guard rails.
- **Pair-review agent**: A `reviewer-pair.md` agent file with a collaborative, question-asking persona, spawned during the implementation phase to review completed PLAN stages incrementally.
- **Updated role skill files**: Coordinator, Product Owner, Architect, Reviewer, roles SKILL.md, and any supporting skill files updated to reflect the new orchestration model and agent-based spawning.
- **Updated xtask tooling**: If `validate-workflow` needs updates to accommodate the new agent file structure, phase definitions, or cross-consultation rules, those changes are in scope.
- **Migration from manual prompt injection to named agent spawning**: The Coordinator's spawning mechanism changes from "paste role markdown into Task prompt" to "spawn named agent via Task tool."

### Out of Scope

- Broader parallel Implementer support (expanding parallel stage execution beyond current capabilities) -- deferred to a follow-up cycle.
- Changes to Claude Code's Task tool itself (we work within its current capabilities).
- Automated model tier recommendations or cost optimization logic.
- Changes to the Maintainer role behavior or the merge/release workflow.
- Changes to the existing CodeRabbit integration or the `Phase: coderabbit` reviewer behavior (though it gets its own agent file).
- Agent configuration for the Coordinator (it runs in the main thread as the top-level agent, not spawned via Task tool).
- `.claude` symlink reusability across projects -- the user noted `.claude` is a symlink to an agents directory, but making this portable/reusable is out of scope for this cycle.
- Per-agent thinking budget (not supported by Claude Code platform; model selection is the workaround).

## Definitions

### Phases

A **phase** is a named operational mode of a role, represented by a separate agent file in `.claude/agents/`. Each phase determines:
1. **Behavioral persona** -- the mindset, focus, and output format the role adopts (defined in skills)
2. **Agent configuration** -- model, tools, permissions, max turns (defined in agent frontmatter)
3. **Trigger context** -- when in the SDLC workflow the Coordinator spawns this agent

A role without phases has a single agent file. A role with phases has one agent file per phase, named `<role>-<phase>.md`.

**Current phase definitions:**

| Agent File | Role | Phase | Persona | Trigger |
|---|---|---|---|---|
| `reviewer-pair.md` | Reviewer | `pair` | Collaborative, curious, asks questions | After each completed PLAN stage during implementation |
| `reviewer-internal.md` | Reviewer | `internal` | Adversarial, thorough, finds problems | After implementation complete, before PR marked ready |
| `reviewer-coderabbit.md` | Reviewer | `coderabbit` | Focused, addresses external findings | After CodeRabbit completes its review |

Other roles currently have no phases (single agent file each). New phases can be added to any role by creating additional agent files following the `<role>-<phase>.md` naming convention.

### Agent File Structure

Each agent file in `.claude/agents/` follows this structure:

```markdown
---
name: <agent-name>
description: <brief description>
model: <sonnet|opus|haiku>
tools:
  - <tool1>
  - <tool2>
  - Task(other-agent-1, other-agent-2)  # agents this agent can spawn
disallowedTools:
  - <tool>
permissionMode: <mode>
maxTurns: <number>
skills:
  - <skill-name>
---

# <Role Name>

<Agent-specific instructions, phase context, or additional behavioral guidance
that supplements the skill content>
```

## Feature Specifications

### Feature 1: Native Agent Configuration via `.claude/agents/`

**Agent files to create (8 total):**

| File | Role | Phase | Notes |
|---|---|---|---|
| `coordinator.md` | Coordinator | -- | Top-level agent; defines which other agents it can spawn via `tools: [Task(...)]`. Not spawned itself but needs the file for tool declarations. |
| `product-owner.md` | Product Owner | -- | Skills: roles/references/product-owner behavioral content |
| `architect.md` | Architect | -- | Skills: roles/references/architect behavioral content |
| `implementer.md` | Implementer | -- | Skills: roles/references/implementer behavioral content; needs file editing and git tools |
| `reviewer-pair.md` | Reviewer | pair | Lightweight pair review persona; may use cheaper model |
| `reviewer-internal.md` | Reviewer | internal | Existing adversarial review persona |
| `reviewer-coderabbit.md` | Reviewer | coderabbit | Existing CodeRabbit response persona |
| `maintainer.md` | Maintainer | -- | Skills: roles/references/maintainer behavioral content; needs git and gh tools |

**Model configuration:**
- Each agent file sets its `model` field in frontmatter
- The user controls which model each agent uses by editing the agent file
- No separate config file needed -- the model lives in the agent's own frontmatter
- No fallback chain needed -- Claude Code handles defaults natively when `model` is omitted

**Tool restrictions per agent:**
- Each agent file defines its `tools` and `disallowedTools` in frontmatter
- The Coordinator's `tools` field lists all agents it can spawn: `Task(product-owner, architect, implementer, reviewer-pair, reviewer-internal, reviewer-coderabbit, maintainer)`
- Implementer: needs file editing, git, terminal tools
- Reviewer variants: should be read-focused (read files, run tests, view diffs) -- specific tool lists determined by Architect
- Product Owner: minimal tools (read files, state file writing)
- Architect: read files, state file writing
- Maintainer: git, gh CLI tools

**Skills injection:**
- Each agent uses the `skills` field to load its behavioral instructions
- Behavioral content stays in `.claude/skills/roles/references/` (or is split appropriately between the skill and the agent markdown body)
- The Architect determines the exact split: what goes in the skill vs what goes in the agent body

**Coordinator spawning change:**
- **Before:** Coordinator reads role `.md` from skills, pastes content into Task tool prompt
- **After:** Coordinator spawns agents by name: `Task(architect, "Design the solution based on REQUIREMENTS.md at .state/<branch>/REQUIREMENTS.md")`
- The Coordinator no longer needs to know or read the role behavioral content -- it just references the agent name

### Feature 2: Bidirectional Cross-Consultation

**Core change:** The Coordinator is now allowed -- and encouraged -- to spawn a secondary agent as a short-lived consultant during another role's active phase. This replaces the strict "one role at a time, fully sequential" model with "one lead role per phase, with targeted consultations."

**Phase structure preserved:**
- The PO still leads the requirements phase and owns REQUIREMENTS.md
- The Architect still leads the design phase and owns ADR.md and PLAN.md
- Each phase still has a clear gate before the next phase begins
- The artifacts (REQUIREMENTS.md, ADR.md, PLAN.md) are unchanged

**What changes:**
- During PO phase: the Coordinator can spawn the `architect` agent for feasibility checks, scope validation, or early design input
- During Architect phase: the Coordinator can spawn the `product-owner` agent to verify alignment with user intent, check that requirements are accurately captured in the design, and adjust requirements if needed
- The user participates actively in both phases (this is unchanged from today, just more dynamic)

**Trigger mechanisms (all three valid):**
1. **Lead role requests it** -- the PO or Architect explicitly asks for a consultation from the other role in their output (e.g., "I recommend checking with the Architect whether this requirement is technically feasible before I finalize")
2. **Coordinator decides** -- the Coordinator recognizes a situation where cross-consultation would prevent downstream rework (e.g., the PO is defining a requirement that has obvious technical implications)
3. **User requests it** -- the user directly asks to bring in the other role's perspective (e.g., "Can we check with the Architect on this?")

**Protocol:**
- Cross-consultation uses the existing Role-to-Role Collaboration Protocol (SKILL.md Section 3): structured request/response format, max 1 active question per role, max 2 follow-ups per question
- Additional guard rail: **max 3 cross-consultations per phase**. After 3 consultations in a single phase, the Coordinator must proceed without further consultation. This prevents infinite ping-pong
- The Coordinator tracks consultation count per phase

**Disagreement resolution:**
- The lead role makes the final call on their artifact. PO owns REQUIREMENTS.md; Architect owns ADR.md/PLAN.md
- If the lead role and consulted role disagree after the 2-follow-up limit, the Coordinator escalates to the user for a decision
- The user's decision is final

**Performance trade-off (acknowledged):**
- Each consultation is a full subagent spawn. Multiple consultations per phase increase total agent invocations
- This is acceptable because better-informed artifacts reduce revision cycles in later phases, which are more expensive (full implementation rework vs a quick consultation)
- Consultation prompts should be focused and minimal

### Feature 3: Three-Phase Review Lifecycle

**`reviewer-pair` agent (NEW)**

Trigger: The Coordinator spawns the `reviewer-pair` agent after each completed PLAN stage during the implementation phase. Specifically:
- The Implementer marks a PLAN stage as complete (`- [x]`)
- The Implementer reports stage completion to the Coordinator
- The Coordinator spawns `reviewer-pair` for that stage

Review scope: The pair reviewer examines:
- The diff for files listed in the completed PLAN stage's `Files` field
- The PLAN stage description and relevant ADR context
- NOT the full PR, NOT uncommitted work, NOT other stages

Output format -- questions, observations, and flags (not formal severity-classified findings):
- **Questions**: "Why was X chosen over Y here?" / "How does this interact with Z?"
- **Observations**: "This pattern differs from how the rest of the codebase handles similar logic"
- **Flags**: "This might conflict with Stage N which modifies the same struct"

Finding classification and flow:
- Pair reviewer findings are reported to the Coordinator
- The Coordinator classifies each finding as **blocking** (must fix before next stage) or **non-blocking** (collect for later)
- Classification criteria: findings that indicate wrong direction, requirement misunderstanding, or will cause cascading rework in later stages are blocking. Style, optimization, and minor pattern concerns are non-blocking
- Blocking findings: the Coordinator sends them to the Implementer, who addresses them before starting the next stage
- Non-blocking findings: collected and included as context for the senior reviewer

**`reviewer-internal` agent (EXISTING -- unchanged behavior)**

Trigger: After the entire implementation is complete, before the PR is marked ready. Behavior unchanged from current workflow.

Additional input: The senior reviewer receives the collected pair review findings as informational context. The senior reviewer:
- Reviews everything independently (full adversarial review from scratch)
- May agree, disagree, or find new issues beyond what pair review caught
- Is NOT bound by pair review conclusions
- Pair review context helps avoid re-flagging already-addressed issues, but does not limit the review scope

**`reviewer-coderabbit` agent (EXISTING -- unchanged behavior)**

Trigger: After CodeRabbit completes its external review. Behavior unchanged from current workflow.

**Pair reviewer does not initiate cross-consultation:**
- If the pair reviewer identifies something that needs Architect or PO input, it reports this to the Coordinator as a flag
- The Coordinator decides whether to spawn a cross-consultation
- The pair reviewer itself does not directly request other roles

## Acceptance Criteria

### Agent Configuration
- [ ] `.claude/agents/` directory exists with 8 agent files (coordinator, product-owner, architect, implementer, reviewer-pair, reviewer-internal, reviewer-coderabbit, maintainer)
- [ ] Each agent file has YAML frontmatter with at minimum: `name`, `description`, `model`
- [ ] Each agent file has appropriate `tools` and/or `disallowedTools` restricting its capabilities to what the role needs
- [ ] Each agent file uses the `skills` field to load behavioral instructions from `.claude/skills/`
- [ ] The Coordinator agent file declares all spawnable agents in its `tools` field via `Task(...)`
- [ ] The Coordinator spawns agents by name (e.g., `Task(architect, ...)`) instead of manually injecting role prompts
- [ ] Removing or omitting the `model` field from an agent file falls back to Claude Code's default behavior (no crash, no special handling needed)
- [ ] Each agent's model can be changed by editing its frontmatter -- no other file needs to change

### Cross-Consultation
- [ ] During the PO phase, the Coordinator can spawn the `architect` agent as a short-lived consultant
- [ ] During the Architect phase, the Coordinator can spawn the `product-owner` agent as a short-lived consultant
- [ ] Consultations are triggered by lead role request, Coordinator judgment, or user request
- [ ] Cross-consultation uses the existing Role-to-Role Collaboration Protocol (structured format, max 2 follow-ups)
- [ ] Max 3 cross-consultations per phase, enforced by the Coordinator
- [ ] Disagreements unresolved after 2 follow-ups are escalated to the user
- [ ] The lead role retains final authority over their artifact (PO owns REQUIREMENTS.md, Architect owns ADR.md/PLAN.md)
- [ ] REQUIREMENTS.md and ADR.md/PLAN.md are still produced as distinct artifacts with clear phase gates between them

### Three-Phase Review
- [ ] `reviewer-pair` agent exists with collaborative, question-asking persona and appropriate lightweight model
- [ ] `reviewer-internal` agent exists with adversarial, thorough persona (preserving current behavior)
- [ ] `reviewer-coderabbit` agent exists with CodeRabbit-response persona (preserving current behavior)
- [ ] `reviewer-pair` is spawned by the Coordinator after each completed PLAN stage during implementation
- [ ] Pair reviewer output uses the questions/observations/flags format (not severity-classified findings)
- [ ] The Coordinator classifies pair review findings as blocking or non-blocking
- [ ] Blocking findings must be addressed by the Implementer before starting the next PLAN stage
- [ ] Non-blocking findings are collected and provided as context to `reviewer-internal`
- [ ] `reviewer-internal` receives pair review context but reviews independently
- [ ] The pair reviewer does not initiate cross-consultation; it reports flags to the Coordinator

### Orchestration and Skill Files
- [ ] The Coordinator role/agent is updated to reflect the new orchestration model (named agent spawning, cross-consultation, pair review lifecycle)
- [ ] The Reviewer skill content is updated or split to support three distinct phase personas
- [ ] The PO and Architect skill content is updated with guidance on when to recommend cross-consultation
- [ ] The roles SKILL.md is updated to reflect the phase concept, agent file structure, and cross-consultation rules
- [ ] All existing xtask validation commands continue to work or are updated to match the new agent file structure
- [ ] The SDLC flow diagram in the Coordinator reflects the new phase structure

## Constraints

- **Claude Code Task tool limitations:** Subagents run to completion and return a result. They cannot persist or wait for input. Cross-consultation and pair review work within this constraint as short-lived spawns, not persistent agents.
- **Backward compatibility of artifacts:** REQUIREMENTS.md, ADR.md, and PLAN.md remain the canonical SDLC outputs. Their format and purpose are unchanged. Only the process of producing them changes.
- **No per-agent thinking budget:** Claude Code does not support per-agent thinking budget configuration. Model selection is the workaround for controlling reasoning depth/cost per agent. This is a known platform limitation.
- **`.claude` symlink reusability out of scope:** The user noted that `.claude` is a symlink to a shared agents directory. Making this portable or reusable across projects is explicitly out of scope for this cycle.
- **No new logical roles:** The total logical role count remains at 6 (Coordinator, PO, Architect, Implementer, Reviewer, Maintainer). The 8 agent files reflect 3 reviewer phase variants, not 3 separate roles. The Reviewer is one role with three operational phases.
- **Agent files are the single source of truth for configuration:** Model, tools, permissions, and skills injection are defined in agent frontmatter. No separate configuration file is needed or created.

## Context

- The roles skill is defined at `.claude/skills/roles/SKILL.md` with role references in `.claude/skills/roles/references/`.
- The Coordinator role at `.claude/skills/roles/references/coordinator.md` currently describes manual prompt injection for spawning -- this will be replaced with named agent spawning.
- `.claude/agents/` does not currently exist and will be created as part of this work.
- Claude Code's `.claude/agents/` mechanism supports YAML frontmatter fields: `name`, `description`, `tools`, `disallowedTools`, `model` (sonnet/opus/haiku), `permissionMode`, `maxTurns`, `skills`, `mcpServers`, `hooks`, `memory`.
- The Task tool can reference agents by name: `Task(agent-name, "prompt")`.
- Existing parallel implementation support (via `cargo xtask coordinate-plan`) is preserved but not expanded in this cycle.
- The Reviewer currently has a `Phase` parameter (`internal`/`coderabbit`) passed in the spawn prompt. This will be replaced by spawning distinct agent files (`reviewer-internal`, `reviewer-coderabbit`, `reviewer-pair`).
- The Role-to-Role Collaboration Protocol (SKILL.md Section 3) already defines structured cross-role communication. Cross-consultation extends this with additional triggers and guard rails rather than inventing a new mechanism.

---
**Sign-off:** Approved by user
