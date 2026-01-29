# Agent Session Recorder (AGR)

```
 █████╗  ██████╗ ██████╗
██╔══██╗██╔════╝ ██╔══██╗
███████║██║  ███╗██████╔╝
██╔══██║██║   ██║██╔══██╗
██║  ██║╚██████╔╝██║  ██║
╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝
 ⏺ REC ───────────────────────────────────────────────────────────────────────────────────────

[ Agent Session Recorder ] - Record, replay, and understand AI agent sessions.
```

[![CI](https://github.com/thiscantbeserious/agent-session-recorder/actions/workflows/ci.yml/badge.svg)](https://github.com/thiscantbeserious/agent-session-recorder/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/thiscantbeserious/agent-session-recorder/graph/badge.svg)](https://codecov.io/gh/thiscantbeserious/agent-session-recorder)
[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![asciinema](https://img.shields.io/badge/powered%20by-asciinema-d40000)](https://asciinema.org/)

**Record, review, and understand your AI agent sessions.**

AGR is a lightweight CLI tool that automatically records your terminal sessions with AI coding assistants like Claude Code, Codex, and Gemini CLI. It uses [asciinema](https://asciinema.org/) under the hood to capture everything - commands, output, timing - so you can replay sessions, analyze what happened, and learn from your AI-assisted coding workflows.

## Features

- **Transparent recording** - Shell wrappers automatically record sessions without changing your workflow
- **AI-powered analysis** - Agents can analyze their own recordings and mark interesting moments
- **Native asciicast v3** - Markers stored directly in `.cast` files, playable in any asciinema player
- **Storage management** - Track usage by agent, get warnings when storage is high, clean up old sessions
- **Interactive TUI** - Browse and preview recordings with a terminal-based interface

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
asciinema play ~/recorded_agent_sessions/claude/session.cast

# Remove long pauses from a recording (e.g., lunch breaks):
agr optimize --remove-silence session.cast

# Add a marker to highlight an important moment:
agr marker add session.cast 45.2 "Build failed - missing dependency"
```

## Post-Processing Recordings

### Silence Removal

Long pauses in recordings (coffee breaks, lunch, thinking time) can make playback tedious. The optimize command removes these silences by capping intervals at a configurable threshold.

```bash
# Use default threshold (2.0 seconds) or header's idle_time_limit:
agr optimize --remove-silence session.cast

# Use a custom threshold (1.5 seconds):
agr optimize --remove-silence=1.5 session.cast

# Write to a new file instead of modifying in-place:
agr optimize --remove-silence --output fast.cast session.cast
```

**Threshold Resolution Order:**
1. CLI argument (`--remove-silence=1.5`) - explicit user intent
2. Header's `idle_time_limit` - recording author's intent
3. Default: 2.0 seconds

## Documentation

| Resource | Description |
|----------|-------------|
| [Command Reference](docs/COMMANDS.md) | Complete CLI documentation |
| [Wiki](../../wiki) | Detailed guides and configuration |
| `agr --help` | Interactive TUI help |
| `agr <command> --help` | Command-specific help |

## Configuration

AGR uses a TOML configuration file at `~/.config/agr/config.toml`.

```bash
agr config show    # View current configuration
agr config edit    # Open in your editor
```

See the [Wiki](../../wiki) for full configuration reference.

## Development

```bash
cargo build --release    # Build
cargo test               # Run tests
./tests/e2e_test.sh      # E2E tests (requires asciinema)
cargo xtask gen-docs     # Regenerate documentation
```

### Project Structure

```
src/
├── main.rs       # CLI entry point
├── cli.rs        # CLI definitions (clap)
├── commands/     # Command handlers
└── tui/          # Terminal UI components
docs/
├── COMMANDS.md   # Generated command reference
├── man/          # Generated man pages
└── wiki/         # Generated wiki pages
xtask/            # Build tasks (doc generation)
```

## License

MIT License - see [LICENSE](LICENSE)
