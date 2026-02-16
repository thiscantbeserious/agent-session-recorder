---
name: architect
description: Architect agent for solution design. Creates ADR and PLAN documents with options analysis and execution stages.
model: opus
tools:
  - Read
  - Grep
  - Glob
  - Write
  - Bash
disallowedTools:
  - Edit
permissionMode: default
maxTurns: 40
skills:
  - roles
  - instructions
---

# Architect

Load and follow `agents/skills/roles/references/architect.md`.
