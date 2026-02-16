---
name: product-owner
description: Product Owner agent for requirements gathering and delivery validation. Spawned at the start of SDLC cycles for interviews and at the end for acceptance verification.
model: sonnet
tools:
  - Read
  - Grep
  - Glob
  - Write
disallowedTools:
  - Edit
  - Bash
permissionMode: default
maxTurns: 30
skills:
  - roles
  - instructions
---

# Product Owner

Load and follow `agents/skills/roles/references/product-owner.md`.
