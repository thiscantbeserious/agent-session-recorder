# Agent Session Recorder (AGR)

```
╔══════════════════════════════════════╗
║  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  ⏺ REC    ║
╠══════════════════════════════════════╣
║   ░█████╗░░██████╗░██████╗░          ║
║   ██╔══██╗██╔════╝░██╔══██╗          ║
║   ███████║██║░░██╗░██████╔╝          ║
║   ██╔══██║██║░░╚██╗██╔══██╗          ║
║   ██║░░██║╚██████╔╝██║░░██║          ║
║   ╚═╝░░╚═╝░╚═════╝░╚═╝░░╚═╝          ║
║   ◉──────────────────────⏺ REC       ║
║   A G E N T   S E S S I O N          ║
║   R E C O R D E R                    ║
╚══════════════════════════════════════╝

[ Agent Session Recorder ] - auto-record agent sessions and handle the recordings with AI!

Usage: agr <COMMAND>

Commands:
  record   Start recording a session
  status   Show storage statistics
  cleanup  Interactive cleanup of old sessions
  list     List recorded sessions [aliases: ls]
  analyze  Analyze a recording with AI
  marker   Manage markers in cast files
  agents   Manage configured agents
  config   Configuration management
  shell    Manage shell integration
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help (see more with '--help')
  -V, --version  Print version
```

[![CI](https://github.com/thiscantbeserious/agent-session-recorder/actions/workflows/ci.yml/badge.svg)](https://github.com/thiscantbeserious/agent-session-recorder/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/thiscantbeserious/agent-session-recorder/graph/badge.svg)](https://codecov.io/gh/thiscantbeserious/agent-session-recorder)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![asciinema](https://img.shields.io/badge/powered%20by-asciinema-d40000)](https://asciinema.org/)

**Record, review, and understand your AI agent sessions.**

AGR is a lightweight CLI tool that automatically records your terminal sessions with AI coding assistants like Claude Code, Codex, and Gemini CLI. It uses [asciinema](https://asciinema.org/) under the hood to capture everything - commands, output, timing - so you can replay sessions, analyze what happened, and learn from your AI-assisted coding workflows.

## Why AGR?

When working with AI coding assistants, sessions can be long and complex. You might want to:

- **Review what happened** - Replay a session to understand how a problem was solved
- **Track usage** - See how much you're using different AI tools and manage storage
- **Mark key moments** - Add markers at important points (errors, breakthroughs, decisions)
- **Learn and improve** - Study successful sessions to refine your prompting techniques

AGR handles the recording automatically and transparently - just use your AI tools as normal.

## Features

- **Transparent recording** - Shell wrappers automatically record sessions without changing your workflow
- **AI-powered analysis** - Agents can analyze their own recordings and mark interesting moments
- **Native asciicast v3** - Markers stored directly in `.cast` files, playable in any asciinema player
- **Storage management** - Track usage by agent, get warnings when storage is high, clean up old sessions
- **Flexible configuration** - Control which agents are recorded, storage thresholds, and behavior

## Installation

### Prerequisites

- [asciinema](https://asciinema.org/) must be installed (`brew install asciinema` or `apt install asciinema`)

### From Source

```bash
git clone https://github.com/thiscantbeserious/agent-session-record.git
cd agent-session-recorder
./install.sh
```

The installer will:
1. Build the `agr` binary and install it to `~/.local/bin/`
2. Create the config directory at `~/.config/agr/`
3. Create the recordings directory at `~/recorded_agent_sessions/`
4. Set up shell integration in your `.zshrc` or `.bashrc`

### Manual Installation

```bash
cargo build --release
cp target/release/agr ~/.local/bin/
agr shell install
```

## Quick Start

After installation, restart your shell or run `source ~/.zshrc`.

```bash
# Your AI tools now auto-record! Just use them normally:
claude "help me refactor this function"

# Or record manually:
agr record claude

# List your recorded sessions:
agr list

# Check storage usage:
agr status

# Play back a recording:
asciinema play ~/recorded_agent_sessions/claude/20250120-143052.cast

# Add a marker to highlight an important moment:
agr marker add session.cast 45.2 "Build failed - missing dependency"
```

### AI-Powered Analysis

Enable auto-analyze to have an AI agent automatically analyze recordings after each session:

```toml
# In ~/.config/agr/config.toml
[recording]
auto_analyze = true
analysis_agent = "claude"  # or "codex" or "gemini"
```

When enabled, AGR spawns the configured agent after recording to analyze the session and add markers at interesting points (errors, decisions, milestones).

**Manual analysis** (if auto_analyze is disabled):
```bash
# Run AI analysis on an existing recording
agr analyze session.cast

# Or use a different agent than configured
agr analyze session.cast --agent codex

# Or add markers manually
agr marker add session.cast 45.2 "Build failed: missing dependency"
agr marker add session.cast 120.5 "Deployment completed successfully"
```

## Configuration

AGR uses a TOML configuration file at `~/.config/agr/config.toml`. All settings have sensible defaults - you only need to configure what you want to change.

### Full Configuration Reference

```toml
[storage]
# Where recordings are stored (supports ~ expansion)
directory = "~/recorded_agent_sessions"

# Warn when total storage exceeds this size (in GB)
size_threshold_gb = 5.0

# Used by cleanup to identify old sessions (in days)
age_threshold_days = 30

[agents]
# Which agents to track and offer for auto-wrapping
enabled = ["claude", "codex", "gemini"]

# Agents that should NOT be auto-wrapped (record manually with `agr record`)
no_wrap = []

[shell]
# Master switch: set to false to disable all auto-wrapping
auto_wrap = true

[recording]
# Automatically spawn an AI agent to analyze recordings after each session
auto_analyze = false

# Which agent CLI to use for analysis (must be installed)
analysis_agent = "claude"  # or "codex" or "gemini"
```

### Configuration Options Explained

#### `[storage]` Section

| Option | Default | Description |
|--------|---------|-------------|
| `directory` | `~/recorded_agent_sessions` | Base directory for all recordings. Each agent gets a subdirectory (e.g., `~/recorded_agent_sessions/claude/`). |
| `size_threshold_gb` | `5.0` | When total storage exceeds this, AGR shows a warning after each recording suggesting cleanup. |
| `age_threshold_days` | `30` | Sessions older than this are shown first in `agr cleanup` for easy removal. |

#### `[agents]` Section

| Option | Default | Description |
|--------|---------|-------------|
| `enabled` | `["claude", "codex", "gemini"]` | List of agent commands to track. Add any CLI tool you want to record. |
| `no_wrap` | `[]` | Agents in this list won't be auto-wrapped even if in `enabled`. Use this to disable auto-recording for specific tools while keeping them in the enabled list. |

#### `[shell]` Section

| Option | Default | Description |
|--------|---------|-------------|
| `auto_wrap` | `true` | Master switch for shell auto-wrapping. Set to `false` to disable all automatic recording - you can still record manually with `agr record <agent>`. |

#### `[recording]` Section

| Option | Default | Description |
|--------|---------|-------------|
| `auto_analyze` | `false` | When `true`, automatically spawns an AI agent to analyze the recording after each session. The agent reads the session and adds markers at key moments. |
| `analysis_agent` | `"claude"` | Which agent CLI to use for auto-analysis. Options: `claude`, `codex`, `gemini`. The agent must be installed on your system. |

### Example Configurations

**Minimal - just change storage location:**
```toml
[storage]
directory = "~/my-ai-recordings"
```

**Add a custom agent:**
```toml
[agents]
enabled = ["claude", "codex", "gemini", "aider", "cursor"]
```

**Disable auto-wrap for one agent:**
```toml
[agents]
enabled = ["claude", "codex", "gemini"]
no_wrap = ["codex"]  # codex won't auto-record, but you can still use `agr record codex`
```

**Disable all auto-wrapping (manual recording only):**
```toml
[shell]
auto_wrap = false
```

**Lower storage threshold for aggressive cleanup reminders:**
```toml
[storage]
size_threshold_gb = 2.0
age_threshold_days = 14
```

### Managing Configuration

```bash
# View current configuration
agr config show

# Edit configuration in your default editor
agr config edit

# Manage agents via CLI
agr agents list              # Show enabled agents
agr agents add aider         # Add an agent
agr agents remove codex      # Remove an agent

# Manage no-wrap list
agr agents no-wrap list      # Show agents excluded from auto-wrap
agr agents no-wrap add codex # Exclude an agent from auto-wrap
agr agents no-wrap remove codex  # Re-enable auto-wrap for an agent
```

## Commands Reference

### Recording

| Command | Description |
|---------|-------------|
| `agr record <agent> [-- args]` | Record a session. Args after `--` are passed to the agent. |
| `agr analyze <file> [--agent <name>]` | Analyze a recording with AI and add markers |

**Examples:**
```bash
agr record claude -- --model opus
agr analyze ~/recorded_agent_sessions/claude/session.cast
agr analyze session.cast --agent codex
```

### Session Management

| Command | Description |
|---------|-------------|
| `agr list [agent]` | List all recordings, optionally filtered by agent |
| `agr status` | Show storage statistics with breakdown by agent |
| `agr cleanup` | Interactive cleanup - select how many old sessions to delete |

### Markers

| Command | Description |
|---------|-------------|
| `agr marker add <file> <time> <label>` | Add a marker at the given timestamp (seconds) |
| `agr marker list <file>` | List all markers in a recording |

**Example:**
```bash
agr marker add session.cast 120.5 "Found the bug!"
agr marker list session.cast
```

### Agent Configuration

| Command | Description |
|---------|-------------|
| `agr agents list` | Show configured agents |
| `agr agents add <name>` | Add an agent to the enabled list |
| `agr agents remove <name>` | Remove an agent from the enabled list |
| `agr agents is-wrapped <name>` | Check if an agent will be auto-wrapped (exit 0=yes, 1=no) |
| `agr agents no-wrap list` | Show agents excluded from auto-wrapping |
| `agr agents no-wrap add <name>` | Exclude an agent from auto-wrapping |
| `agr agents no-wrap remove <name>` | Re-enable auto-wrapping for an agent |

### Shell Integration

| Command | Description |
|---------|-------------|
| `agr shell status` | Show if shell integration is installed and where |
| `agr shell install` | Add shell integration to your RC file |
| `agr shell uninstall` | Remove shell integration from your RC file |

### Configuration

| Command | Description |
|---------|-------------|
| `agr config show` | Display current configuration |
| `agr config edit` | Open config file in your default editor |

## Shell Integration

AGR's shell integration creates wrapper functions for your configured agents. When you run `claude`, the wrapper:

1. Checks if auto-wrap is enabled (globally and for this agent)
2. Checks if already inside a recording (to avoid nesting)
3. If both pass, runs `agr record claude` instead of `claude` directly

### Manual Setup

If you prefer manual setup instead of `agr shell install`:

```bash
# Add to ~/.zshrc or ~/.bashrc
source ~/.config/agr/agr.sh
```

### How It Works

The shell integration adds a marked section to your RC file:

```bash
# >>> AGR (Agent Session Recorder) >>>
# DO NOT EDIT - managed by 'agr shell install/uninstall'
export _AGR_LOADED=1
[ -f "$HOME/.config/agr/agr.sh" ] && source "$HOME/.config/agr/agr.sh"
# <<< AGR (Agent Session Recorder) <<<
```

This makes it easy to update or remove with `agr shell uninstall`.

## asciicast v3 Format

AGR uses the native [asciicast v3](https://docs.asciinema.org/manual/asciicast/v3/) format with marker support:

```json
{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","$ echo hello\r\n"]
[1.0,"m","Important moment"]
[0.1,"o","hello\r\n"]
```

- Output events use type `"o"` with terminal data
- Marker events use type `"m"` with a label string
- Timestamps are relative intervals (time since previous event)

Recordings are fully compatible with asciinema's player and tools.

## Development

### Building

```bash
# Build locally
cargo build --release

# Run tests
cargo test

# Run E2E tests (requires asciinema)
./tests/e2e_test.sh

# Build with Docker (Linux binary)
./build.sh
```

### Project Structure

```
src/
├── main.rs       # CLI entry point (clap)
├── lib.rs        # Library root
├── config.rs     # TOML configuration
├── asciicast.rs  # v3 parser/writer
├── markers.rs    # Marker injection
├── storage.rs    # Storage management
├── recording.rs  # asciinema wrapper
└── analyzer.rs   # Auto-analysis with AI agents
```

### Cross-Compilation

```bash
# macOS targets
rustup target add x86_64-apple-darwin aarch64-apple-darwin
cargo build --release --target aarch64-apple-darwin

# Linux (requires cross-compiler or use Docker)
cargo build --release --target x86_64-unknown-linux-musl
```

## Uninstalling

```bash
./uninstall.sh
```

Or manually:
```bash
agr shell uninstall
rm ~/.local/bin/agr
rm -rf ~/.config/agr
rm -rf ~/recorded_agent_sessions  # if you want to delete recordings
```

## License

MIT License - see [LICENSE](LICENSE)
