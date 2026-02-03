# agr copy

Copy a recording to the clipboard

## Usage

```
agr copy [OPTIONS] <FILE>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | Path to the .cast recording file |

## Description

Copy a recording file to the system clipboard.

On macOS, the file is copied as a file reference, allowing direct paste
into Slack, email, or other applications as an attachment. On Linux,
the file content is copied as text (file copy not supported).

Recordings can be specified by absolute path, short format (agent/file.cast),
or just filename (fuzzy matches across all agents).

EXAMPLES:
    agr copy session.cast                 Copy by filename (fuzzy match)
    agr copy claude/session.cast          Copy using short format
    agr copy /path/to/session.cast        Copy by absolute path

