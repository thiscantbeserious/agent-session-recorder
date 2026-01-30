# agr list

List recorded sessions

## Usage

```
agr list [OPTIONS] [AGENT]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `AGENT` | Filter sessions by agent name |

## Description

List all recorded sessions with details.

Shows sessions sorted by date (newest first) with agent name,
age, file size, and filename.

EXAMPLES:
    agr list                List all sessions
    agr ls                  Same as 'agr list' (alias)
    agr list claude         List only Claude sessions
    agr list codex          List only Codex sessions

