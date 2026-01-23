# AGR Command Reference

This document is auto-generated from the CLI definitions.

## Table of Contents

- [record](#agr-record)
- [status](#agr-status)
- [cleanup](#agr-cleanup)
- [list](#agr-list)
- [analyze](#agr-analyze)
- [marker](#agr-marker)
- [agents](#agr-agents)
- [config](#agr-config)
- [shell](#agr-shell)

---

## agr

[ Agent Session Recorder ] - auto-record agent sessions and handle the recordings with AI!

```
Agent Session Recorder (AGR) - Record AI agent terminal sessions with asciinema.

AGR automatically records your AI coding agent sessions (Claude, Codex, Gemini, etc.)
to ~/recorded_agent_sessions/ in asciicast v3 format. Recordings can be played back
with asciinema, auto-analyzed by AI agents, and annotated with markers.

QUICK START:
    agr record claude              Record a Claude session
    agr status                     Check storage usage
    agr list                       List all recordings
    agr cleanup                    Clean up old recordings

SHELL INTEGRATION:
    agr shell install              Auto-record configured agents
    agr agents add claude          Add agent to auto-record list

For more information, see: https://github.com/thiscantbeserious/agent-session-recorder
```

## agr record

Start recording a session

### Arguments

- `<AGENT>`: Agent name (e.g., claude, codex, gemini)
- `<ARGS>`: Arguments to pass to the agent (after --)

### Options

- `-n, --name`: Session name (skips rename prompt)

### Description

```
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
```

---

## agr status

Show storage statistics

### Description

```
Display storage statistics for recorded sessions.

Shows total size, disk usage percentage, session count by agent,
and age of the oldest recording.

EXAMPLE:
    agr status

OUTPUT:
    Agent Sessions: 1.2 GB (0.5% of disk)
       Sessions: 23 total (claude: 15, codex: 8)
       Oldest: 2025-01-01 (20 days ago)
```

---

## agr cleanup

Interactive cleanup of old sessions

### Options

- `--agent`: Only show sessions from this agent
- `--older-than`: Only show sessions older than N days

### Description

```
Interactively delete old session recordings to free up disk space.

Displays a list of sessions sorted by age and lets you choose how many
to delete. Supports filtering by agent and age. Sessions older than
the configured threshold (default: 30 days) are marked with *.

EXAMPLES:
    agr cleanup                          Interactive cleanup of all sessions
    agr cleanup --agent claude           Only show Claude sessions
    agr cleanup --older-than 60          Only show sessions older than 60 days
    agr cleanup --agent codex --older-than 30

INTERACTIVE OPTIONS:
    [number]    Delete the N oldest sessions
    'old'       Delete all sessions older than threshold
    'all'       Delete all matching sessions
    0           Cancel without deleting
```

---

## agr list

List recorded sessions

### Arguments

- `<AGENT>`: Filter sessions by agent name

### Description

```
List all recorded sessions with details.

Shows sessions sorted by date (newest first) with agent name,
age, file size, and filename.

EXAMPLES:
    agr list                List all sessions
    agr ls                  Same as 'agr list' (alias)
    agr list claude         List only Claude sessions
    agr list codex          List only Codex sessions
```

---

## agr analyze

Analyze a recording with AI

### Arguments

- `<FILE>`: Path to the .cast recording file

### Options

- `-a, --agent`: Agent to use (overrides config)

### Description

```
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
```

---

## agr marker

Manage markers in cast files

### Description

```
Add and list markers in asciicast recording files.

Markers are annotations at specific timestamps in a recording,
useful for highlighting key moments like errors, decisions, or
milestones. Markers use the native asciicast v3 marker format.

EXAMPLES:
    agr marker add session.cast 45.2 "Build failed"
    agr marker add session.cast 120.5 "Deployment complete"
    agr marker list session.cast
```

### Subcommands

#### agr marker add

Add a marker to a cast file at a specific timestamp

- `<FILE>`: Path to the .cast recording file
- `<TIME>`: Timestamp in seconds (e.g., 45.2)
- `<LABEL>`: Description of the marker (e.g., "Build failed")

```
Add a marker to a cast file at a specific timestamp.

Markers are injected into the asciicast file using the native v3 marker
format. The timestamp is cumulative seconds from the start of the recording.

EXAMPLE:
    agr marker add ~/recorded_agent_sessions/claude/session.cast 45.2 "Build error"
```

#### agr marker list

List all markers in a cast file

- `<FILE>`: Path to the .cast recording file

```
List all markers in a cast file with their timestamps and labels.

EXAMPLE:
    agr marker list ~/recorded_agent_sessions/claude/session.cast

OUTPUT:
    Markers:
      [45.2s] Build error
      [120.5s] Deployment complete
```

---

## agr agents

Manage configured agents

### Description

```
Manage the list of AI agents that AGR knows about.

Configured agents are used by shell integration to automatically
record sessions. You can also control which agents are auto-wrapped
using the no-wrap subcommand.

EXAMPLES:
    agr agents list                  Show configured agents
    agr agents add claude            Add claude to the list
    agr agents remove codex          Remove codex from the list
    agr agents no-wrap add claude    Disable auto-wrap for claude
```

### Subcommands

#### agr agents list

List all configured agents

```
List all agents configured for recording.

These agents can be auto-recorded when shell integration is enabled.
```

#### agr agents add

Add an agent to the configuration

- `<NAME>`: Name of the agent (e.g., claude, codex)

```
Add an agent to the configured list.

Once added, the agent can be auto-recorded via shell integration.

EXAMPLE:
    agr agents add claude
    agr agents add my-custom-agent
```

#### agr agents remove

Remove an agent from the configuration

- `<NAME>`: Name of the agent to remove

```
Remove an agent from the configured list.

The agent will no longer be auto-recorded via shell integration.

EXAMPLE:
    agr agents remove codex
```

#### agr agents is-wrapped

Check if an agent should be wrapped (used by shell integration)

- `<NAME>`: Name of the agent to check

```
Check if an agent should be auto-wrapped by shell integration.

Returns exit code 0 if the agent should be wrapped, 1 if not.
Used internally by the shell integration script.

EXAMPLE:
    agr agents is-wrapped claude && echo "Should wrap"
```

#### agr agents no-wrap

Manage agents that should not be auto-wrapped

```
Manage the no-wrap list for agents that should not be auto-recorded.

Agents on this list will not be automatically wrapped by shell integration,
even if they are in the configured agents list. Useful for temporarily
disabling recording for specific agents.
```

---

## agr config

Configuration management

### Description

```
View and edit the AGR configuration file.

Configuration is stored in ~/.config/agr/config.toml and includes
storage settings, agent list, shell integration options, and more.

EXAMPLES:
    agr config show          Display current configuration
    agr config edit          Open config in $EDITOR
```

### Subcommands

#### agr config show

Show current configuration as TOML

```
Display the current configuration in TOML format.

Shows all settings including storage paths, agent list, shell options,
and recording preferences.

EXAMPLE:
    agr config show
```

#### agr config edit

Open configuration file in your default editor

```
Open the configuration file in your default editor.

Uses the $EDITOR environment variable (defaults to 'vi').
Config file location: ~/.config/agr/config.toml

EXAMPLE:
    agr config edit
    EDITOR=nano agr config edit
```

---

## agr shell

Manage shell integration

### Description

```
Manage automatic session recording via shell integration.

Shell integration adds wrapper functions to your shell that automatically
record sessions when you run configured agents. It modifies your .zshrc
or .bashrc with a clearly marked section.

EXAMPLES:
    agr shell status         Check if shell integration is installed
    agr shell install        Install shell integration
    agr shell uninstall      Remove shell integration

After installing, restart your shell or run: source ~/.zshrc
```

### Subcommands

#### agr shell status

Show shell integration status

```
Show the current status of shell integration.

Displays whether shell integration is installed, which RC file
is configured, and whether auto-wrap is enabled.

EXAMPLE:
    agr shell status
```

#### agr shell install

Install shell integration to .zshrc/.bashrc

```
Install shell integration for automatic session recording.

Adds a clearly marked section to your .zshrc (or .bashrc) that
sources the AGR shell script. This creates wrapper functions for
configured agents that automatically record sessions.

After installation, restart your shell or run:
    source ~/.zshrc

EXAMPLE:
    agr shell install
```

#### agr shell uninstall

Remove shell integration from .zshrc/.bashrc

```
Remove shell integration from your shell configuration.

Removes the AGR section from your .zshrc/.bashrc and deletes
the shell script. Restart your shell after uninstalling.

EXAMPLE:
    agr shell uninstall
```

---


*Generated by `cargo xtask gen-docs`*
