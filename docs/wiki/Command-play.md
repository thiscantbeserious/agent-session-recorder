# agr play

Play a recording with the native player

## Usage

```
agr play [OPTIONS] <FILE>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | Path to the .cast recording file |

## Description

Play an asciicast recording using the native player.

The native player supports seeking, speed control, and marker navigation.
Recordings can be specified by absolute path, short format (agent/file.cast),
or just filename (fuzzy matches across all agents).

EXAMPLES:
    agr play session.cast                 Play by filename (fuzzy match)
    agr play claude/session.cast          Play using short format
    agr play /path/to/session.cast        Play by absolute path

PLAYER CONTROLS:
    q, Esc      Quit
    Space       Pause/resume
    +/-         Adjust playback speed
    <, > or ,, .  Seek backward/forward 5s
    m           Jump to next marker
    ?           Show help overlay

