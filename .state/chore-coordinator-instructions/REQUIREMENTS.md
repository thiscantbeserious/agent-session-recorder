# Requirements: Refine Role/Startup/Coordinator Behavior

## Problem Statement
Current role/skill orchestration behavior is not consistently aligned with intended workflow. Startup options, `instructions` auto-loading, and coordinator SDLC spawning behavior need to be explicitly enforced and consistent across `AGENTS.md` and role skill definitions.

## Desired Outcome
Agent behavior is deterministic and matches the intended SDLC workflow:
- Startup presents a consistent two-option selection menu by default.
- `instructions` auto-loads only for relevant task types.
- Coordinator correctly starts and progresses an SDLC cycle when selected.
- The non-SDLC path is framed as `Direct Assist` with natural escalation to SDLC for complex tasks.

## Scope
### In Scope
- Update and align behavior rules for startup flow in role/coordinator instructions.
- Enforce exactly two startup options when no role is explicitly assigned.
- Define startup behavior as menu-first unless user intent is explicit.
- Rename the second startup option from Q&A wording to `Direct Assist`.
- Define and enforce when `instructions` skill auto-loads.
- Ensure coordinator behavior correctly initiates SDLC flow when user selects SDLC.
- In `Direct Assist`, propose starting an SDLC cycle (and spawn Product Owner) when the task appears complex.
- Define explicit complexity triggers used to propose SDLC in `Direct Assist`.
- Define a hard transition rule: no role spawning from `Direct Assist` without user confirmation.
- Enforce naming consistency (`Direct Assist`) across `AGENTS.md`, `roles/SKILL.md`, and coordinator role instructions.
- Keep `AGENTS.md` and role/skill instructions consistent for the above behavior.

### Out of Scope
- Adding new roles or redesigning SDLC phases.
- Broad prompt/style changes unrelated to startup/options/auto-load/coordinator flow.
- Feature work outside instruction/behavior alignment.

## Acceptance Criteria
- [ ] On fresh start with no explicit role, the assistant proposes exactly two options:
      1) Start SDLC workflow
      2) Direct Assist (no SDLC yet)
- [ ] Startup is menu-first by default; if user intent is explicit (e.g., asks to start SDLC, asks a direct question, or requests a specific role), the assistant skips the menu and executes directly.
- [ ] No extra startup menu/options are shown unless the user explicitly asks.
- [ ] If SDLC is selected, coordinator starts SDLC flow and spawns the required cycle behavior.
- [ ] If Direct Assist is selected, assistant responds directly without immediately spawning roles.
- [ ] In Direct Assist, when the task appears complex, assistant proposes starting an SDLC cycle and, on confirmation, spawns Product Owner.
- [ ] Complexity triggers are explicitly defined and include at least: multi-file change, architecture/design decision needed, unclear acceptance criteria, or elevated regression risk.
- [ ] In Direct Assist, roles are never spawned unless the user confirms SDLC transition.
- [ ] `instructions` skill auto-loads for coding, testing, git operations, command execution, SDLC artifact handling, or codebase exploration.
- [ ] `instructions` skill does not auto-load for pure Direct Assist interactions where it is not relevant.
- [ ] `Direct Assist` terminology is used consistently with no leftover `Q&A mode` wording in updated instruction files.
- [ ] `AGENTS.md` and skill instructions reflect the same behavior without contradiction.

## Constraints
- Target branch: `main`
- Requirements file path: `.state/chore-coordinator-instructions/REQUIREMENTS.md`
- Keep changes tightly scoped to behavior/rules alignment requested above.

## Context
- User explicitly requested refinement of roles skill and `AGENTS.md` behavior.
- User explicitly requested correct coordinator behavior that spawns SDLC cycle.
- This cycle is focused on capturing and validating requirements for that behavior alignment.

---
**Sign-off:** Pending
