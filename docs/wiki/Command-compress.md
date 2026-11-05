# agr compress

Compress plain sessions to .cast.gz

## Usage

```
agr compress [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--agent` | Only compress sessions from this agent |
| `-y, --yes` | Skip confirmation prompt |
| `--dry-run` | Only show the preview, never prompt or change anything |

## Description

Compress existing plain .cast sessions to .cast.gz.

Shows a preview with the number of sessions, current size and projected
size, then asks for confirmation. Sessions that are currently being
recorded are skipped. Without a TTY nothing is prompted and nothing is
rewritten, use --yes to apply.

EXAMPLES:
    agr compress                    Preview and ask for confirmation
    agr compress --agent claude     Only compress Claude sessions
    agr compress --yes              Compress without confirmation
    agr compress --dry-run          Only show the preview, change nothing

Ctrl+C stops after the current file and prints what was done so far.

