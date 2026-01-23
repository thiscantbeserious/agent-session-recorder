# agr marker

Manage markers in cast files

## Usage

```
agr marker [OPTIONS]
```

## Description

Add and list markers in asciicast recording files.

Markers are annotations at specific timestamps in a recording,
useful for highlighting key moments like errors, decisions, or
milestones. Markers use the native asciicast v3 marker format.

EXAMPLES:
    agr marker add session.cast 45.2 "Build failed"
    agr marker add session.cast 120.5 "Deployment complete"
    agr marker list session.cast

## Subcommands

### marker add

Add a marker to a cast file at a specific timestamp

Add a marker to a cast file at a specific timestamp.

Markers are injected into the asciicast file using the native v3 marker
format. The timestamp is cumulative seconds from the start of the recording.

EXAMPLE:
    agr marker add ~/recorded_agent_sessions/claude/session.cast 45.2 "Build error"

### marker list

List all markers in a cast file

List all markers in a cast file with their timestamps and labels.

EXAMPLE:
    agr marker list ~/recorded_agent_sessions/claude/session.cast

OUTPUT:
    Markers:
      [45.2s] Build error
      [120.5s] Deployment complete

