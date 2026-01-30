# agr optimize

Optimize asciicast recordings (removes silence)

## Usage

```
agr optimize [OPTIONS] <FILE>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | Path to the .cast recording file |

## Options

| Option | Description |
|--------|-------------|
| `--remove-silence` | Cap intervals at threshold (default: header or 2.0s) |
| `-o, --output` | Output file path |

## Description

Optimize asciicast recording files by removing silence.

Optimization modifies the timing of recordings by capping long pauses
at a configurable threshold.

THRESHOLD RESOLUTION:
    1. CLI argument (explicit user intent)
    2. Header's idle_time_limit (recording author's intent)
    3. Default: 2.0 seconds

EXAMPLES:
    agr optimize --remove-silence session.cast
        Use header's idle_time_limit or default 2.0s threshold

    agr optimize --remove-silence=1.5 session.cast
        Use explicit 1.5s threshold (note: requires = for value)

    agr optimize --remove-silence --output fast.cast session.cast
        Write to separate file, preserving original

