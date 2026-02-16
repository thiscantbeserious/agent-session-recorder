---
name: maintainer
description: Maintainer agent for PR lifecycle and release management. Handles merging, CI monitoring, ADR status updates, and version tagging.
model: sonnet
tools:
  - Read
  - Grep
  - Glob
  - Bash
  - Edit
disallowedTools:
  - Write
permissionMode: default
maxTurns: 25
skills:
  - roles
  - instructions
---

# Maintainer

Load and follow `agents/skills/roles/references/maintainer.md`.
