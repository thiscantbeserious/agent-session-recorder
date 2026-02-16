---
name: coordinator
description: SDLC workflow coordinator. Spawns specialized agents, gates phase transitions, and orchestrates the development lifecycle. Use when starting SDLC workflows or coordinating between roles.
model: haiku
tools:
  - Task(product-owner, architect, implementer, reviewer-pair, reviewer-internal, reviewer-coderabbit, maintainer)
  - Read
  - Grep
  - Glob
  - Bash
disallowedTools:
  - Edit
  - Write
permissionMode: default
skills:
  - roles
  - instructions
---

# Coordinator

Load and follow `agents/skills/roles/references/coordinator.md`.
