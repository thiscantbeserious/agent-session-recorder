# ADR: Shell Integration Architecture - Option C

**Date:** 2026-01-30
**Status:** Accepted
**Decision:** Fully Dynamic with Caching (Option C)

---

## Context

### Current Problems

1. **Static Command Lists**: The completion scripts (`completions.bash`, `completions.zsh`) contain hardcoded command lists that are out of sync with the CLI. Missing commands: `play`, `optimize`, `ls` (alias), `completions` (hidden).

2. **Dual Maintenance Burden**: Two separate completion scripts (~150 lines each) that duplicate logic and must be manually kept in sync with `cli.rs`.

3. **No Ghost Text Support**: Users want Fish/zsh-autosuggestions style inline suggestions showing the most likely file completion with "greyish letters and slight background" styling.

4. **Verbose Embedded Scripts**: The full shell script (~128 lines with comments) is embedded verbatim in RC files, creating visual clutter.

5. **Monolithic Code**: `src/shell.rs` (338 lines) handles installation, uninstallation, path detection, status, and completions - too many responsibilities.

### Options Considered

| Option | Description | Trade-offs |
|--------|-------------|------------|
| **A: Fully Static** | Regenerate completion files at build time from clap | Stale until reinstall, no ghost text |
| **B: Hybrid** | Static commands + dynamic file completions | Better but still stale commands |
| **C: Fully Dynamic** | Shell init embeds commands from clap at install time, dynamic file lookups | ~20ms startup overhead, always fresh |

---

## Decision

**Selected: Option C - Fully Dynamic with Caching**

The shell initialization script will be generated dynamically at install time using `agr completions --shell-init <shell>`. This command:

1. Outputs shell code with the command list embedded directly (extracted from clap at runtime)
2. Includes ghost text hooks that call `agr completions --files` for file suggestions
3. Is minified before embedding into RC files

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Install Time                                 │
├─────────────────────────────────────────────────────────────────────┤
│  agr shell install                                                   │
│       │                                                              │
│       ▼                                                              │
│  agr completions --shell-init zsh  ─────►  Generated shell code     │
│       │                                     (commands from clap)     │
│       ▼                                                              │
│  minify()  ─────────────────────────────►  Compact shell code       │
│       │                                                              │
│       ▼                                                              │
│  Embed in ~/.zshrc between markers                                   │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                         Runtime (Shell)                              │
├─────────────────────────────────────────────────────────────────────┤
│  User types: agr play <space>                                        │
│       │                                                              │
│       ▼                                                              │
│  ZLE widget / bind -x triggers                                       │
│       │                                                              │
│       ▼                                                              │
│  agr completions --files --limit 1  ─────►  Most recent .cast file  │
│       │                                                              │
│       ▼                                                              │
│  Display as ghost text: \e[90;100m<suggestion>\e[0m                 │
└─────────────────────────────────────────────────────────────────────┘
```

### Command Embedding

Commands extracted from clap at install time:
- `record` - Start recording a session
- `status` - Show storage statistics
- `cleanup` - Interactive cleanup of old sessions
- `list` / `ls` - List recorded sessions
- `play` - Play a recording with native player
- `analyze` - Analyze a recording with AI
- `optimize` - Remove silence from recordings
- `marker` - Manage markers (subcommands: add, list)
- `agents` - Manage configured agents (subcommands: list, add, remove, is-wrapped, no-wrap)
- `config` - Configuration management (subcommands: show, edit, migrate)
- `shell` - Manage shell integration (subcommands: status, install, uninstall)
- `completions` - Internal completion helper (hidden)

### File-Accepting Commands

These commands trigger ghost text file suggestions:
- `agr play <file>` - .cast file
- `agr analyze <file>` - .cast file
- `agr optimize <file>` - .cast file
- `agr marker add <file>` - .cast file
- `agr marker list <file>` - .cast file

### Ghost Text Implementation

**Zsh (ZLE widgets + POSTDISPLAY):**
```zsh
# Widget triggered on space after file-accepting command
_agr_ghost() {
    if [[ "$BUFFER" =~ ^agr\ (play|analyze|optimize)\ $ ]]; then
        local suggestion=$(agr completions --files --limit 1 2>/dev/null)
        if [[ -n "$suggestion" ]]; then
            POSTDISPLAY=$'\e[90;100m'"$suggestion"$'\e[0m'
        fi
    fi
}
zle -N _agr_ghost
# Bind to show ghost on space, clear on other keys
```

**Bash (bind -x + cursor manipulation):**
```bash
# Similar pattern using READLINE_LINE and cursor positioning
_agr_ghost() {
    if [[ "$READLINE_LINE" =~ ^agr\ (play|analyze|optimize)\ $ ]]; then
        local suggestion=$(agr completions --files --limit 1 2>/dev/null)
        # Display ghost using cursor save/restore
    fi
}
bind -x '"\x20": _agr_ghost'  # Space triggers ghost
```

**Styling:** `\e[90;100m` = Gray text (90) + dim background (100)

---

## Consequences

### Positive

- **Single Source of Truth**: Commands always match CLI definition (extracted from clap)
- **No Stale Completions**: Reinstalling updates everything automatically
- **Native Ghost Text**: First-class feature, not bolted on
- **Cleaner RC Files**: Minified script reduces visual clutter
- **Better Maintainability**: Modular Rust code, no duplicate shell scripts
- **Delete Old Files**: Remove `shell/completions.bash` and `shell/completions.zsh`

### Negative

- **~20ms Startup Overhead**: Shell init runs `agr completions --shell-init` (acceptable per user)
- **External Binary Dependency**: Completions fail if `agr` binary not in PATH
- **Complexity**: Ghost text requires shell-specific implementations (ZLE vs bind -x)

### Neutral

- **Upgrade Path**: Auto-migrate old installations by detecting marker and regenerating
- **Graceful Degradation**: Ghost text silently fails if `agr` not available

---

## Module Structure

Refactor `src/shell.rs` (338 lines) into modular components:

```
src/shell/
├── mod.rs           # Public API re-exports
│   - pub use install::*;
│   - pub use status::*;
│   - pub use paths::*;
│   - pub use minify::*;
│   - pub use completions::*;
│
├── install.rs       # RC file installation/uninstallation
│   - install(rc_file: &Path) -> io::Result<()>
│   - uninstall(rc_file: &Path) -> io::Result<bool>
│   - generate_section() -> String
│   - MARKER_START, MARKER_END, MARKER_WARNING
│
├── status.rs        # Status detection and reporting
│   - ShellStatus struct
│   - get_status(auto_wrap_enabled: bool) -> ShellStatus
│   - is_installed_in(rc_file: &Path) -> io::Result<bool>
│   - find_installed_rc() -> Option<PathBuf>
│   - extract_script_path(rc_file: &Path) -> io::Result<Option<PathBuf>>
│
├── paths.rs         # Path detection (RC files, completion dirs)
│   - detect_shell_rc() -> Option<PathBuf>
│   - all_shell_rcs() -> Vec<PathBuf>
│   - bash_completion_path() -> Option<PathBuf>
│   - zsh_completion_path() -> Option<PathBuf>
│   - default_script_path() -> Option<PathBuf>
│
├── minify.rs        # Script minification logic (NEW)
│   - minify(script: &str) -> String
│   - Removes comments (except shebang)
│   - Removes blank lines
│   - Preserves strings containing #
│
└── completions.rs   # Completion generation (REWRITE)
    - generate_shell_init(shell: Shell) -> String
    - Commands embedded from clap
    - Ghost text hooks
    - Two-layer completion logic
```

### Key Implementation Details

**minify.rs:**
```rust
pub fn minify(script: &str) -> String {
    script
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Keep non-empty, non-comment lines
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n")
}
```

**completions.rs - generate_shell_init():**
```rust
pub fn generate_shell_init(shell: Shell) -> String {
    // Extract commands from clap
    let commands = extract_commands_from_clap();

    match shell {
        Shell::Zsh => generate_zsh_init(&commands),
        Shell::Bash => generate_bash_init(&commands),
    }
}

fn extract_commands_from_clap() -> Vec<CommandInfo> {
    // Use clap's introspection to get command names and descriptions
    // This is the single source of truth
}
```

---

## Migration Path

1. **Detection**: Check for existing AGR markers in RC file
2. **Backup**: Optional backup of RC file before modification
3. **Uninstall**: Remove old section between markers
4. **Reinstall**: Generate new minified section with embedded commands
5. **Cleanup**: Delete old completion files from `~/.zsh/completions/_agr` and `~/.local/share/bash-completion/completions/agr`

Auto-migration happens automatically on `agr shell install` if old installation detected.

---

## References

- REQUIREMENTS.md - Full requirements document
- src/cli.rs - Clap CLI definitions (source of truth for commands)
- shell/agr.sh - Current shell integration script
- shell/completions.{bash,zsh} - Files to be deleted
