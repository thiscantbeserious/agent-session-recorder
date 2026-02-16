---
name: reviewer-internal
description: Adversarial internal reviewer for thorough post-implementation review. Performs full code analysis, security review, and ADR compliance check before PR is marked ready.
model: opus
tools:
  - Read
  - Grep
  - Glob
  - Bash
disallowedTools:
  - Edit
  - Write
permissionMode: default
maxTurns: 40
skills:
  - roles
  - instructions
---

# Internal Reviewer

Load and follow `agents/skills/roles/references/reviewer.md` with Phase: internal.
