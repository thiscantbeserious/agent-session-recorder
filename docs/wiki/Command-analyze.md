# agr analyze

Analyze a recording with AI

## Usage

```
agr analyze [OPTIONS] <FILE>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | Path to the .cast recording file |

## Options

| Option | Description |
|--------|-------------|
| `-a, --agent` | Agent to use: claude, codex, gemini |
| `-w, --workers` | Number of parallel workers |
| `-t, --timeout` | Timeout per chunk in seconds |
| `--no-parallel` | Disable parallel processing |
| `--curate` | Auto-curate to 8-12 markers without prompting |
| `--debug` | Enable debug mode (required for --output) |
| `-o, --output` | Save cleaned content and exit (optionally specify filename) |
| `--fast` | Skip JSON schema enforcement (faster but less reliable) |
| `--wait` | Wait for keypress before exiting (used by TUI) |

## Description

Analyze a recording file using an AI agent.

The analyzer reads the cast file, extracts meaningful content (removing ANSI
codes and noise), and uses AI to identify key engineering moments. Markers
are added directly to the file using the native asciicast v3 format.

For large files, analysis is parallelized across multiple chunks, with
automatic retry and rate limit handling.

The default agent is configured in ~/.config/agr/config.toml under
[analysis].agent. Use --agent to override for a single run.

EXAMPLES:
    agr analyze session.cast                     Analyze with default agent
    agr analyze session.cast --agent codex       Use Codex instead
    agr analyze session.cast --workers 4         Use 4 parallel workers
    agr analyze session.cast --no-parallel       Sequential mode
    agr analyze session.cast --timeout 180       3 minute timeout per chunk

SUPPORTED AGENTS:
    claude      Claude Code CLI (default)
    codex       OpenAI Codex CLI
    gemini      Google Gemini CLI

