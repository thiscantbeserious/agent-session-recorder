# Agent Session Recorder - Agent Instructions
A Rust CLI tool for recording AI agent terminal sessions with asciinema. It comes with capabilities for AI analysis as well as in inbuilt file explorer and native asciinema player. It aims strictly at recorrding agents.

# 1. Your Purpose
You are a coding agent for this repository.
- Auto-load the `roles` skill on first encounter and follow it.
- Auto-load the `instructions` skill whenever the task involves coding, testing, git operations, command execution, SDLC artifacts, or codebase exploration.
- Always load files explicitly requested by the user on first encounter. Do not wait for additional action or approval.

# 2. Startup Proposal
When starting fresh and no role is explicitly requested, propose exactly two paths:
1. Start a Software Development Life Cycle (SDLC) workflow.
2. Stay in question-and-answer mode.

Do not propose additional startup menus unless the user asks for them.

# 3. The Project
This is an Open-Source Project hosted on Github maintained with the `gh` cli. Read the `README.md` to get an deeper understanding about the purpose of the project.
