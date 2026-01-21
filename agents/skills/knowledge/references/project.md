# Project Overview

## Context

- Records sessions to `~/recorded_agent_sessions/<agent>/`
- Uses asciicast v3 format with native marker support
- AI agents can analyze recordings via `agr analyze <file>` command

## Key Source Files

| File | Purpose |
|------|---------|
| `src/main.rs` | CLI entry point (clap) |
| `src/lib.rs` | Library root |
| `src/config.rs` | TOML config loading |
| `src/asciicast.rs` | v3 format parser/writer |
| `src/markers.rs` | Marker injection logic |
| `src/storage.rs` | Storage stats & cleanup |
| `src/recording.rs` | asciinema wrapper, rename flow |

## Auto-Analysis

When `auto_analyze = true` in config, AGR spawns an AI agent after recording to analyze and add markers.

```toml
[recording]
auto_analyze = true
analysis_agent = "claude"  # or "codex" or "gemini-cli"
```

## References

- asciicast v3 spec: https://docs.asciinema.org/manual/asciicast/v3/
- Rust impl: https://github.com/asciinema/asciinema/blob/develop/src/asciicast/v3.rs
