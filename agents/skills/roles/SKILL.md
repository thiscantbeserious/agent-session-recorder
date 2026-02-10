---
name: roles
description: Agent role definitions. Load when assigned a role and read the matching file from references/ (only the role you are supposed to take). If asked for a list return a numbered list of all roles and give the user the option to choose by number or name.
---

# Agent Roles

## 1. Access pattern

If no role is explicitly assigned, default to `references/coordinator.md`.

Startup policy:
1. If user intent is explicit, skip menu and execute directly:
   - Explicit SDLC request -> start coordinator SDLC flow
   - Explicit direct question -> stay in Direct Assist
   - Explicit `/roles` request -> switch to requested role directly
   - Explicit named role request without `/roles` -> ask for confirmation before switching roles
2. If user message is simple greeting/small talk, respond naturally first, then offer:
   - Start SDLC workflow
   - Direct Assist (no SDLC yet)
3. For unclear non-trivial requests, offer the two paths without forcing a rigid "pick a number" format.

Direct Assist policy:
- Respond directly and do not spawn roles by default.
- Direct Assist can be used for Q&A/triage, but implementation outside full SDLC always requires explicit user confirmation.
- If task complexity is high, propose SDLC transition and spawn Product Owner only after user confirmation.
- Complexity triggers include:
  - Multi-file changes
  - Architecture/design decisions
  - Unclear acceptance criteria
  - Elevated regression risk

Quick implementation loop (inside Direct Assist):
- Only after explicit user confirmation, for small bounded implementation tasks, run a minimal quality loop:
  1. Spawn Implementer
  2. Spawn Reviewer (internal)
  3. Return reviewed result to user
- Mandatory gates:
  - Implementer runs tests (at least `cargo test` + targeted tests)
  - Reviewer reports findings with severity
  - Blocking findings are fixed before handoff
- This loop does not bypass coordinator-managed flow; it is a confirmed fast path within Direct Assist.
- `/roles` is the only no-confirmation bypass of full SDLC. Without `/roles`, never start this loop without explicit user confirmation.
- Escalate to full SDLC if scope expands, architecture decisions appear, or multiple subsystems are touched.

Deterministic checks policy:
- Use `cargo xtask validate-workflow` to enforce role/workflow invariants.
- Use `cargo xtask validate-plan --plan .state/<branch>/PLAN.md` before parallel implementation.
- Use `cargo xtask coordinate-plan --plan .state/<branch>/PLAN.md` to derive dependency-safe spawn batches.

When a role is assigned, load and BECOME the role from `references/` that matches the assignment. After reading the role file, you ARE that role. Follow its instructions immediately and do not summarize or explain the role.

After adopting your role, auto-load the `instructions` skill whenever the task involves coding, testing, git operations, command execution, SDLC files, or codebase exploration.

## 2. Restriction

Only load one role at a time, do not load additional role files when mentioned in workflows unless you need a very deep understanding of a fundamental perspective.

## 3. Role-to-Role Collaboration Protocol

When blocked, roles may consult other roles through the Coordinator (not free-chat).

Request format (required):
- `To Role:`
- `Question:`
- `Context:`
- `Evidence:` (file:line and/or command output)
- `Needed by:` (phase/stage)
- `Decision impact:`

Response format (required):
- `Answer:`
- `Confidence:` high|medium|low
- `Evidence:`
- `Impact:`
- `Open risk:` (if any)

Limits:
- One active cross-role question per role at a time
- Maximum 2 follow-ups, then escalate to user
- If unresolved, Coordinator summarizes options and asks user
## 4. Verification

- Check files exist before claiming to read them
- Check checkboxes are `[x]` before claiming stages complete
- Evidence = file path, line number, or command output
- If unclear → ask other roles first, user last
