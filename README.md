# Agent Session Recorder (ASR)

A small command-line tool that uses [asciinema](https://asciinema.org/) to track all AI agent sessions, leveraging the agents themselves to create markers at interesting key points automatically, in addition to keeping track of total usage.

## Features

- **Automatic session recording**: Transparent shell wrappers record Claude, Codex, Gemini CLI sessions without changing your workflow
- **AI-powered markers**: Agents analyze their own recordings and mark interesting moments (errors, decisions, milestones)
- **Native asciicast v3 format**: Markers stored directly in `.cast` files, compatible with asciinema player
- **Storage management**: Track usage, view stats by agent, clean up old sessions interactively
- **Configurable**: Enable/disable agents, set storage thresholds, customize behavior via TOML config

## Installation

### From Source

```bash
git clone https://github.com/thiscantbeserious/agent-session-record.git
cd agent-session-record
./install.sh
```

### With Homebrew (coming soon)

```bash
brew tap thiscantbeserious/tap
brew install asr
```

## Quick Start

```bash
# Record a Claude session
asr record claude

# List recorded sessions
asr list

# Check storage usage
asr status

# Add a marker to a recording
asr marker add session.cast 45.2 "Build failed here"

# Analyze a session (in Claude/Codex/Gemini)
/asr-analyze ~/recorded_agent_sessions/claude/session.cast
```

## Commands

| Command | Description |
|---------|-------------|
| `asr record <agent> [-- args]` | Start recording a session |
| `asr list [agent]` | List recorded sessions |
| `asr status` | Show storage statistics |
| `asr cleanup` | Interactive cleanup of old sessions |
| `asr marker add <file> <time> <label>` | Add a marker at timestamp |
| `asr marker list <file>` | List markers in a file |
| `asr agents list` | List configured agents |
| `asr agents add <name>` | Add an agent to config |
| `asr config show` | Display current configuration |
| `asr config edit` | Edit configuration file |

## Configuration

Configuration file: `~/.config/asr/config.toml`

```toml
[storage]
directory = "~/recorded_agent_sessions"
size_threshold_gb = 5
age_threshold_days = 30

[agents]
enabled = ["claude", "codex", "gemini-cli"]
```

## Shell Integration

Add to your `.zshrc` or `.bashrc`:

```bash
source /path/to/agent-session-recorder/shell/asr.sh
```

This creates wrapper functions for configured agents that automatically record sessions.

## asciicast v3 Format

ASR uses the native [asciicast v3](https://docs.asciinema.org/manual/asciicast/v3/) marker format:

```json
{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","$ echo hello\r\n"]
[1.0,"m","Important moment"]
[0.1,"o","hello\r\n"]
```

Marker events use type `"m"` with the label as data.

## Development

### Building

```bash
# Build with Docker (runs tests)
./build.sh

# Output binary: dist/asr
```

### Testing

```bash
cargo test
```

### Project Structure

```
src/
├── main.rs       # CLI entry point
├── lib.rs        # Library root
├── config.rs     # Configuration
├── asciicast.rs  # v3 parser/writer
├── markers.rs    # Marker injection
├── storage.rs    # Storage management
└── recording.rs  # Recording logic
```

## License

MIT License - see [LICENSE](LICENSE)
