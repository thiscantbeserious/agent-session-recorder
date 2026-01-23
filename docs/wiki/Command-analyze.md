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
| `-a, --agent` | Agent to use (overrides config) |

## Description

Analyze a recording file using an AI agent.

The analyzer reads the cast file, identifies key moments (errors, decisions,
milestones), and adds markers using 'agr marker add'. This is the same
analysis that runs automatically when auto_analyze is enabled.

The default agent is configured in ~/.config/agr/config.toml under
[recording].analysis_agent. Use --agent to override for a single run.

EXAMPLES:
    agr analyze session.cast              Analyze with default agent
    agr analyze session.cast --agent codex    Override agent for this run

SUPPORTED AGENTS:
    claude      Claude Code CLI
    codex       OpenAI Codex CLI
    gemini  Google Gemini CLI

