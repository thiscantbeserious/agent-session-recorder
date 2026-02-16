---
name: implementer
description: Implementer agent for code changes. Works from PLAN stages, follows TDD, creates PRs. Spawned per implementation task.
model: sonnet
tools:
  - Read
  - Write
  - Edit
  - Grep
  - Glob
  - Bash
permissionMode: acceptEdits
maxTurns: 75
skills:
  - roles
  - instructions
---

# Implementer

Load and follow `agents/skills/roles/references/implementer.md`.
