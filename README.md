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
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS-informational)]()
[![Quality Gate](https://sonarcloud.io/api/project_badges/measure?project=thiscantbeserious_agent-session-recorder&metric=alert_status)](https://sonarcloud.io/summary/new_code?id=thiscantbeserious_agent-session-recorder)
[![Maintainability](https://sonarcloud.io/api/project_badges/measure?project=thiscantbeserious_agent-session-recorder&metric=sqale_rating)](https://sonarcloud.io/summary/new_code?id=thiscantbeserious_agent-session-recorder)
[![Security](https://sonarcloud.io/api/project_badges/measure?project=thiscantbeserious_agent-session-recorder&metric=security_rating)](https://sonarcloud.io/summary/new_code?id=thiscantbeserious_agent-session-recorder)
[![Reliability](https://sonarcloud.io/api/project_badges/measure?project=thiscantbeserious_agent-session-recorder&metric=reliability_rating)](https://sonarcloud.io/summary/new_code?id=thiscantbeserious_agent-session-recorder)
[![Code Smells](https://sonarcloud.io/api/project_badges/measure?project=thiscantbeserious_agent-session-recorder&metric=code_smells)](https://sonarcloud.io/summary/new_code?id=thiscantbeserious_agent-session-recorder)


<video src="https://github.com/user-attachments/assets/e83cf993-467f-4869-b413-f86ddfae1fad" width="600" autoplay loop muted playsinline></video>

**Record, replay, and understand your AI agent sessions.**

AGR is a lightweight CLI tool that automatically records your terminal sessions with AI coding assistants like Claude Code, Codex, and Gemini CLI. It uses [asciinema](https://asciinema.org/) under the hood to capture everything - commands, output, timing - so you can replay sessions, analyze what happened, and learn from your AI-assisted coding workflows.

## Features

- **Transparent recording** - Shell wrappers automatically record sessions without changing your workflow
- **AI-powered analysis** - Agents can analyze their own recordings and mark interesting moments
- **Native asciicast v3** - Markers stored directly in the recording, playable in any asciinema player
- **Compressed by default** - Recordings are stored as `.cast.gz`, about 10x smaller
- **Storage management** - Track usage by agent, get warnings when storage is high, clean up old sessions
- **Interactive TUI** - Browse and preview recordings with a terminal-based interface

## Installation

### Prerequisites

**asciinema v3+** is required (v2 won't work)

```bash
cargo install asciinema
asciinema --version  # must be 3.x
```

### From Source

```bash
git clone https://github.com/thiscantbeserious/agent-session-recorder.git
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

# Play back a recording with the native player:
agr play session.cast

# Copy a recording to the clipboard (paste into Slack, email, etc.):
agr copy session.cast

# Remove long pauses from a recording (e.g., lunch breaks):
agr optimize --remove-silence session.cast

# Add a marker to highlight an important moment:
agr marker add session.cast 45.2 "Build failed - missing dependency"
```

## Playing Recordings

AGR includes a native player with seeking, speed control, and marker navigation.

```bash
# Play by filename (fuzzy matches across agents)
agr play session.cast

# Play using short format
agr play claude/session.cast

# Play by absolute path
agr play ~/recorded_agent_sessions/claude/session.cast
```

### Player Controls

| Key | Action |
|-----|--------|
| `Space` | Pause / Resume |
| `+` / `-` | Adjust playback speed |
| `<` / `>` or `,` / `.` | Seek backward/forward 5s |
| `Home` / `End` | Go to start/end |
| `m` | Jump to next marker |
| `v` | Toggle viewport mode (for large recordings) |
| `r` | Resize terminal to match recording |
| `?` | Show help overlay |
| `q` / `Esc` | Quit player |

**Viewport Mode**: When the recording is larger than your terminal, press `v` to enter viewport mode. Use arrow keys to scroll around the recording, and press `Esc` to exit viewport mode.

## Copying Recordings

Copy recordings to your clipboard for sharing via Slack, email, or other applications.

```bash
# Copy by filename (fuzzy matches across agents)
agr copy session.cast

# Copy using short format
agr copy claude/session.cast

# Copy by absolute path
agr copy ~/recorded_agent_sessions/claude/session.cast
```

**Platform Behavior:**
- **macOS**: Copies as file reference (POSIX file) - paste directly into Slack, email, etc. as an attachment
- **Linux**: Copies as file URI (`text/uri-list`) - works in modern DEs (GNOME, KDE) for file paste

## Interactive File Browser

Use `agr list` or `agr ls` to open the interactive TUI for browsing recordings.

### Browser Controls

| Key | Action |
|-----|--------|
| `Enter` | Open context menu |
| `p` | Play selected recording |
| `c` | Copy recording to clipboard |
| `r` | Rename recording |
| `t` | Optimize (remove silence) |
| `a` | Analyze recording with AI |
| `d` | Delete recording |
| `/` | Search by filename |
| `f` | Filter by agent |
| `Esc` | Cancel / clear filters |
| `?` | Show help overlay |
| `q` | Quit browser |

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

## Storage and Compression

New recordings are saved gzip-compressed as `.cast.gz`. Every command (play, list, analyze, optimize, marker, copy, cleanup) works the same on `.cast.gz` and plain `.cast` files, and `agr copy` always hands over plain asciicast.

```toml
[storage]
compression = "gzip"   # or "none" to keep writing plain .cast
```

`agr status` and `agr list` show the size on disk and the uncompressed size.

### Compressing Existing Sessions

```bash
agr compress                  # Preview sessions, current vs projected size, then ask
agr compress --agent claude   # Only one agent
agr compress --yes            # Skip the confirmation
agr compress --dry-run        # Only show the preview, never prompt or change anything
```

Sessions that are currently being recorded are skipped. Without a TTY nothing is rewritten unless you pass `--yes`. On a terminal a progress line shows the current file and the size done so far on stderr. Ctrl+C stops after the current file and prints what was done.

Orphaned backups (a hidden `.bak` whose session is gone) and stale lock files are shown by `agr status` and removed by `agr cleanup` after a preview and confirmation (`--yes` skips it).

After an upgrade, `agr list`, `agr ls`, `agr status` and `agr cleanup` offer the same preview once per version, interactive terminals only. The last version that showed the offer is kept in `~/.config/agr/state.toml`. Nothing is rewritten without confirmation.

### Reading a Session with Standard Tools

```bash
gzip -dc session.cast.gz | less
```

Use `gzip -dc` and not `zcat`, on macOS `zcat` is the legacy `.Z` tool.

### Colors

Set `NO_COLOR=1` to disable colored output.

## Syncing Sessions

Sync copies your compressed sessions to a second place so another machine can pull them. Right now the only backend is secret GitHub gists through the `gh` CLI, as such `gh` and `git` need to be on your PATH and `gh auth login` done (the token needs the `gist` scope, `gh auth refresh -s gist` adds it). agr never reads or stores your GitHub token.

> **Not verified against real GitHub yet.** The gist transport (`gh gist create`, the `git` push of the binary file, the list call and the error mapping) is only tested with a fake process runner and a fake backend in this PR. No real gist was created, edited or deleted. Treat the first real run as a test.

### Setup

Sync is off until you set a backend in `~/.config/agr/config.toml`:

```toml
[sync]
backend = "gist"
auto_push = true   # default, only has an effect once a backend is set
```

```bash
agr sync status                 # What push and pull would do, per session, read only
agr sync push                   # Upload new and changed sessions
agr sync pull                   # Download new and changed sessions
agr sync delete claude/a.cast.gz  # Remove the remote copy, the local file stays
agr sync push --dry-run         # Preview, never prompts or changes anything
agr sync push --yes             # Skip the confirmation (needed without a terminal)
```

Every session is its own gist (`agent/name.cast.gz`). Plain `.cast` files are skipped with a hint to run `agr compress`, sessions that are still recording are skipped, and so are sessions above 100MB on disk. A conflict (both sides changed, decided by content hash and never by file time) is skipped and reported, `--overwrite` makes the local side win on push and the remote side win on pull (the replaced local file is kept as a hidden `.bak`). Deleting a session locally never deletes it remotely and the other way round, only `agr sync delete` removes the remote copy. A later plain `agr sync push` uploads a deleted session again, as the local file is still there.

### Encryption and the key

Secret gists are not listed anywhere, but **anyone who has the link can read them**, and recordings can hold secrets you typed in the terminal. That is why agr encrypts every session on your machine before it uploads (XChaCha20-Poly1305). The gist only holds ciphertext, and its description only holds an opaque token, so session names are not readable either. A gist keeps its git history, as such old ciphertexts of a changed session stay readable to anyone with the link and the key.

The first push that creates gists shows this notice and asks for a confirmation once per machine (stored in `~/.config/agr/state.toml`). The key is created then, in `~/.config/agr/sync.key` with mode 0600, but only if the remote has no agr gists yet. If it has, agr stops and asks you to import the key of your other machine.

**Losing every copy of the key makes your synced sessions unrecoverable.** Back it up right after the first push:

```bash
agr sync key export             # Prints the recovery phrase (agr1-XXXXX-...)
```

On a second machine:

```bash
agr sync key import             # Paste the phrase on stdin, an argument also works but lands in your shell history
agr sync pull
```

`key import` refuses to replace a different existing key unless you pass `--force`. Remote entries sealed with another key are reported and left alone, remove them with `gh gist delete <id>` if you do not need them.

### Auto push

Once a backend is set, a finished recording is pushed in the background by default. Only that one session is pushed, never a pull or a delete, and a failure never changes the exit status of the recording. Auto push waits until you confirmed the privacy notice with one manual `agr sync push`, until then the recording prints one hint line. A failed background push is stored in `state.toml`, shown as one warning on the next recording and by `agr sync status`.

To turn it off:

```toml
[sync]
auto_push = false
```

## Documentation

| Resource | Description |
|----------|-------------|
| [Command Reference](docs/COMMANDS.md) | Complete CLI documentation |
| [Wiki](../../wiki) | Detailed guides and configuration |
| `agr --help` | CLI help |
| `agr <command> --help` | Command-specific help |

## Configuration

AGR uses a TOML configuration file at `~/.config/agr/config.toml`.

```bash
agr config show    # View current configuration
agr config edit    # Open in your editor
```

### Filename Templates

Customize how recording filenames are generated using template tags:

```toml
[recording]
filename_template = "{directory}{?branch}_{id}"  # Default
directory_max_length = 14                         # Truncate long directory names
```

**Available tags:**

| Tag | Description | Example Output |
|-----|-------------|----------------|
| `{directory}` | Current working directory name | `my-project` |
| `{?branch}` | Optional git branch (wrapped in parens) | `(feature--login)` or empty |
| `{branch}` | Git branch name (sanitized) | `feature--login` |
| `{id}` | Base36 epoch timestamp (7 chars, sortable) | `0ta73n2` |
| `{date}` | Date in YYMMDD format | `260129` |
| `{date:FORMAT}` | Date with custom strftime | `{date:%Y-%m-%d}` → `2026-01-29` |
| `{time}` | Time in HHMM format | `1430` |
| `{time:FORMAT}` | Time with custom strftime | `{time:%H:%M}` → `14:30` |

**Example configurations:**

```toml
# Default: project(feature--login)_0ta73n2.cast
filename_template = "{directory}{?branch}_{id}"

# Branch + date: project_main_260129.cast
filename_template = "{directory}_{branch}_{date}"

# ISO date: project_2026-01-29.cast
filename_template = "{directory}_{date:%Y-%m-%d}"

# Simple ID only: 0ta73n2.cast
filename_template = "{id}"
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
