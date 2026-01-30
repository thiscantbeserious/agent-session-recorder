# agr record

Start recording a session

## Usage

```
agr record [OPTIONS] <AGENT> [ARGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `AGENT` | Agent name (e.g., claude, codex, gemini) |
| `ARGS` | Arguments to pass to the agent (after --) |

## Options

| Option | Description |
|--------|-------------|
| `-n, --name` | Session name (skips rename prompt) |

## Description

Start recording an AI agent session with asciinema.

The recording is saved to ~/recorded_agent_sessions/<agent>/<timestamp>.cast
in asciicast v3 format. When the session ends, you can optionally rename
the recording for easier identification.

EXAMPLES:
    agr record claude                    Record a Claude Code session
    agr record codex                     Record an OpenAI Codex session
    agr record claude --name my-session  Record with a specific filename
    agr record claude -- --help          Pass --help flag to claude
    agr record gemini -- chat        Start gemini in chat mode

