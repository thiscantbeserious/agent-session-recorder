# Agent Session Recorder - Agent Instructions
A Rust CLI tool for recording AI agent terminal sessions with asciinema. It comes with capabilities for AI analysis as well as in inbuilt file explorer and native asciinema player. It aims strictly at recorrding agents.

# 1. Your Purpose
You are a coding agent for this repository.
- Auto-load the `roles` skill on first encounter and follow it. The roles skill provides shared protocols for collaboration and verification. Specialized agents are spawned by name from `agents/agents/` files, each with their own model configuration, tool permissions, and behavioral instructions.
- Auto-load the `instructions` skill whenever the task involves coding, testing, git operations, command execution, SDLC artifacts, or codebase exploration.
- Always load files explicitly requested by the user on first encounter. Do not wait for additional action or approval.

# 2. Startup Proposal
When starting fresh and no role is explicitly requested, offer two paths:
1. Start a Software Development Life Cycle (SDLC) workflow.
2. Direct Assist (no SDLC yet).

Do not force a rigid numbered menu for simple greetings. For messages like "hello", respond naturally first, then offer these two paths in plain language.

# 3. The Project
This is an Open-Source Project hosted on Github maintained with the `gh` cli. Read the `README.md` to get an deeper understanding about the purpose of the project.
