# Project Overview

## Context

- Records sessions to `~/recorded_agent_sessions/<agent>/`
- Uses asciicast v3 format with native marker support
- AI agents can analyze recordings via `agr analyze <file>` command

## Source Code

| Path | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point - clap definitions and command dispatch |
| `src/lib.rs` | Library root - re-exports all modules |
| `src/commands/` | CLI command handlers (one file per command) |
| `tests/unit/` | Unit tests for library modules |
| `tests/e2e/` | End-to-end shell script tests |

Explore `src/` for domain modules (config, storage, recording, etc.).

## Auto-Analysis

When `auto_analyze = true` in config, AGR spawns an AI agent after recording to analyze and add markers.

```toml
[recording]
auto_analyze = true
analysis_agent = "claude"
```

See `src/analyzer.rs` for supported analysis agents.

## References

- asciicast v3 spec: https://docs.asciinema.org/manual/asciicast/v3/
- Rust impl: https://github.com/asciinema/asciinema/blob/develop/src/asciicast/v3.rs
