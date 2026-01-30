# Shell Integration Refactor - Implementation Plan

**Date:** 2026-01-30
**Architecture:** Option C - Fully Dynamic with Caching
**ADR:** See `ADR.md` for architectural decisions

---

## Overview

This plan breaks the refactor into 6 stages, each testable independently. Stages can be committed separately and the system remains functional after each stage.

---

## Stage 1: Refactor `src/shell.rs` into Modules (REQ-4)

**Goal:** Split monolithic shell.rs into focused modules without changing behavior.

### Tasks

1. **Create module structure:**
   ```
   src/shell/
   ├── mod.rs
   ├── install.rs
   ├── status.rs
   ├── paths.rs
   └── completions.rs
   ```

2. **Move code to `paths.rs`:**
   - `detect_shell_rc() -> Option<PathBuf>`
   - `all_shell_rcs() -> Vec<PathBuf>`
   - `bash_completion_path() -> Option<PathBuf>`
   - `zsh_completion_path() -> Option<PathBuf>`
   - `default_script_path() -> Option<PathBuf>`

3. **Move code to `status.rs`:**
   - `ShellStatus` struct and impl
   - `get_status(auto_wrap_enabled: bool) -> ShellStatus`
   - `is_installed_in(rc_file: &Path) -> io::Result<bool>`
   - `find_installed_rc() -> Option<PathBuf>`
   - `extract_script_path(rc_file: &Path) -> io::Result<Option<PathBuf>>`
   - Constants: `MARKER_START`, `MARKER_END`

4. **Move code to `install.rs`:**
   - `install(rc_file: &Path) -> io::Result<()>`
   - `uninstall(rc_file: &Path) -> io::Result<bool>`
   - `generate_section() -> String`
   - `install_script(script_path: &Path) -> io::Result<()>`
   - Constant: `MARKER_WARNING`
   - Embedded script: `SHELL_SCRIPT`

5. **Move code to `completions.rs`:**
   - `install_bash_completions() -> io::Result<Option<PathBuf>>`
   - `install_zsh_completions() -> io::Result<Option<PathBuf>>`
   - `uninstall_bash_completions() -> io::Result<bool>`
   - `uninstall_zsh_completions() -> io::Result<bool>`
   - Embedded completions: `BASH_COMPLETIONS`, `ZSH_COMPLETIONS`

6. **Create `mod.rs`:**
   - Re-export all public items
   - Ensure `agr::shell::*` API unchanged

### Verification

```bash
cargo build
cargo test
# All existing tests must pass
# No API changes visible to consumers
```

### Files Changed
- `src/shell.rs` -> deleted
- `src/shell/mod.rs` -> new
- `src/shell/install.rs` -> new
- `src/shell/status.rs` -> new
- `src/shell/paths.rs` -> new
- `src/shell/completions.rs` -> new

---

## Stage 2: Add Minification (REQ-2)

**Goal:** Compress shell script when embedding into RC files.

### Tasks

1. **Create `src/shell/minify.rs`:**
   ```rust
   //! Shell script minification
   //!
   //! Removes comments and blank lines from shell scripts while
   //! preserving functionality.

   /// Minify a shell script by removing comments and blank lines.
   ///
   /// Preserves:
   /// - Shebang lines (#!/...)
   /// - Lines containing # inside strings (approximation: # not at line start)
   /// - All functional code
   ///
   /// Removes:
   /// - Comment-only lines (starting with #, except shebang)
   /// - Blank/whitespace-only lines
   pub fn minify(script: &str) -> String {
       script
           .lines()
           .filter(|line| {
               let trimmed = line.trim();
               if trimmed.is_empty() {
                   return false;
               }
               // Keep shebang
               if trimmed.starts_with("#!") {
                   return true;
               }
               // Remove comment-only lines
               if trimmed.starts_with('#') {
                   return false;
               }
               true
           })
           .collect::<Vec<_>>()
           .join("\n")
   }

   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_removes_comments() {
           let input = "# comment\necho hello\n# another";
           assert_eq!(minify(input), "echo hello");
       }

       #[test]
       fn test_removes_blank_lines() {
           let input = "echo a\n\n\necho b";
           assert_eq!(minify(input), "echo a\necho b");
       }

       #[test]
       fn test_preserves_shebang() {
           let input = "#!/bin/bash\n# comment\necho hi";
           assert_eq!(minify(input), "#!/bin/bash\necho hi");
       }

       #[test]
       fn test_preserves_inline_hash() {
           let input = "echo \"#hashtag\"\n# comment";
           assert_eq!(minify(input), "echo \"#hashtag\"");
       }
   }
   ```

2. **Update `install.rs` to use minification:**
   ```rust
   use super::minify::minify;

   pub fn generate_section() -> String {
       let minified = minify(SHELL_SCRIPT);
       format!("{MARKER_START}\n{MARKER_WARNING}\n{minified}\n{MARKER_END}")
   }
   ```

3. **Add debug mode:**
   ```rust
   pub fn generate_section_with_options(debug: bool) -> String {
       let script = if debug { SHELL_SCRIPT } else { &minify(SHELL_SCRIPT) };
       format!("{MARKER_START}\n{MARKER_WARNING}\n{script}\n{MARKER_END}")
   }
   ```

4. **Export from `mod.rs`:**
   ```rust
   pub mod minify;
   pub use minify::minify;
   ```

### Verification

```bash
cargo test shell::minify
# Test minification logic

# Manual: Install and verify shell still works
agr shell uninstall
agr shell install
# Open ~/.zshrc, verify minified output
source ~/.zshrc
agr --help  # Should work
```

### Files Changed
- `src/shell/minify.rs` -> new
- `src/shell/mod.rs` -> updated
- `src/shell/install.rs` -> updated

---

## Stage 3: Implement `agr completions --shell-init` (REQ-1)

**Goal:** Generate shell init code with commands embedded from clap.

### Tasks

1. **Update CLI in `src/cli.rs`:**
   ```rust
   /// Generate shell completions (internal use)
   #[command(hide = true)]
   Completions {
       /// Shell to generate completions for (clap native)
       #[arg(long, value_enum)]
       shell: Option<CompletionShell>,

       /// Output shell initialization code with embedded completions
       #[arg(long, value_enum)]
       shell_init: Option<CompletionShell>,

       /// List cast files for completion
       #[arg(long)]
       files: bool,

       /// Limit number of files returned
       #[arg(long, default_value = "10")]
       limit: usize,

       /// Filter prefix for file listing
       #[arg(default_value = "")]
       prefix: String,
   }
   ```

2. **Create command extraction in `src/shell/completions.rs`:**
   ```rust
   use crate::cli::{Cli, Commands};
   use clap::CommandFactory;

   /// Information about a CLI command for completion
   pub struct CommandInfo {
       pub name: String,
       pub description: String,
       pub subcommands: Vec<CommandInfo>,
       pub accepts_file: bool,
   }

   /// Extract command information from clap definitions
   pub fn extract_commands() -> Vec<CommandInfo> {
       let cmd = Cli::command();
       cmd.get_subcommands()
           .map(|sub| CommandInfo {
               name: sub.get_name().to_string(),
               description: sub.get_about()
                   .map(|s| s.to_string())
                   .unwrap_or_default(),
               subcommands: extract_subcommands(sub),
               accepts_file: is_file_accepting(sub.get_name()),
           })
           .collect()
   }

   fn is_file_accepting(cmd: &str) -> bool {
       matches!(cmd, "play" | "analyze" | "optimize" | "marker")
   }
   ```

3. **Generate zsh init code:**
   ```rust
   pub fn generate_zsh_init() -> String {
       let commands = extract_commands();
       let cmd_list = commands
           .iter()
           .map(|c| format!("'{}'", c.name))
           .collect::<Vec<_>>()
           .join(" ");

       format!(r#"
   # AGR Shell Integration - Zsh
   export _AGR_LOADED=1
   _agr_commands=({cmd_list})

   # Two-layer completion: commands first, then files
   _agr_complete() {{
       local cur=${{words[CURRENT]}}
       local cmd=${{words[2]}}

       if (( CURRENT == 2 )); then
           _describe 'commands' _agr_commands
       elif [[ " play analyze optimize " =~ " $cmd " ]]; then
           local files=($(agr completions --files "$cur" --limit 20 2>/dev/null))
           _describe 'cast files' files
       elif [[ "$cmd" == "marker" ]] && (( CURRENT == 4 )); then
           local files=($(agr completions --files "$cur" --limit 20 2>/dev/null))
           _describe 'cast files' files
       fi
   }}
   compdef _agr_complete agr
   "#)
   }
   ```

4. **Generate bash init code:**
   ```rust
   pub fn generate_bash_init() -> String {
       let commands = extract_commands();
       let cmd_list = commands
           .iter()
           .map(|c| c.name.as_str())
           .collect::<Vec<_>>()
           .join(" ");

       format!(r#"
   # AGR Shell Integration - Bash
   export _AGR_LOADED=1
   _agr_commands="{cmd_list}"

   _agr_complete() {{
       local cur="${{COMP_WORDS[COMP_CWORD]}}"
       local cmd="${{COMP_WORDS[1]}}"

       if [[ $COMP_CWORD -eq 1 ]]; then
           COMPREPLY=($(compgen -W "$_agr_commands" -- "$cur"))
       elif [[ " play analyze optimize " =~ " $cmd " ]]; then
           local files=$(agr completions --files "$cur" --limit 20 2>/dev/null)
           COMPREPLY=($(compgen -W "$files" -- "$cur"))
       fi
   }}
   complete -F _agr_complete agr
   "#)
   }
   ```

5. **Implement handler in `src/commands/completions.rs`:**
   ```rust
   if let Some(shell) = args.shell_init {
       let init = match shell {
           Shell::Zsh => crate::shell::completions::generate_zsh_init(),
           Shell::Bash => crate::shell::completions::generate_bash_init(),
           _ => return Err(anyhow!("Unsupported shell for --shell-init")),
       };
       println!("{}", init);
       return Ok(());
   }
   ```

### Verification

```bash
# Test command extraction
cargo test shell::completions::extract

# Test init generation
agr completions --shell-init zsh
agr completions --shell-init bash

# Both should output valid shell code with embedded commands
```

### Files Changed
- `src/cli.rs` -> updated
- `src/shell/completions.rs` -> rewritten
- `src/commands/completions.rs` -> updated (or new)

---

## Stage 4: Implement Ghost Text Hooks (REQ-3)

**Goal:** Add inline ghost text suggestions for file-accepting commands.

### Tasks

1. **Add ghost text to zsh init:**
   ```zsh
   # Ghost text autosuggestion
   _agr_ghost_text=""

   _agr_show_ghost() {
       local buf="$BUFFER"
       _agr_ghost_text=""
       POSTDISPLAY=""

       # Check if we're at a file-accepting position
       if [[ "$buf" =~ ^agr\ (play|analyze|optimize)\ $ ]]; then
           local suggestion=$(agr completions --files --limit 1 2>/dev/null)
           if [[ -n "$suggestion" ]]; then
               _agr_ghost_text="$suggestion"
               # Gray text (90) + dim background (100)
               POSTDISPLAY=$'\e[90;100m'"$suggestion"$'\e[0m'
           fi
       elif [[ "$buf" =~ ^agr\ marker\ (add|list)\ $ ]]; then
           local suggestion=$(agr completions --files --limit 1 2>/dev/null)
           if [[ -n "$suggestion" ]]; then
               _agr_ghost_text="$suggestion"
               POSTDISPLAY=$'\e[90;100m'"$suggestion"$'\e[0m'
           fi
       fi
   }

   _agr_accept_ghost() {
       if [[ -n "$_agr_ghost_text" ]]; then
           BUFFER="$BUFFER$_agr_ghost_text"
           CURSOR=${#BUFFER}
           _agr_ghost_text=""
           POSTDISPLAY=""
       fi
   }

   _agr_self_insert_ghost() {
       zle self-insert
       _agr_show_ghost
   }

   _agr_clear_ghost() {
       _agr_ghost_text=""
       POSTDISPLAY=""
   }

   # Create widgets
   zle -N _agr_self_insert_ghost
   zle -N _agr_accept_ghost
   zle -N _agr_clear_ghost

   # Bind space to trigger ghost display
   bindkey ' ' _agr_self_insert_ghost

   # Tab or Right arrow accepts ghost
   bindkey '^I' _agr_accept_ghost      # Tab
   bindkey '^[[C' _agr_accept_ghost    # Right arrow

   # Any other input clears ghost (handled by precmd)
   precmd() {
       _agr_clear_ghost
   }
   ```

2. **Add ghost text to bash init:**
   ```bash
   # Ghost text for bash (more limited than zsh)
   _agr_ghost_text=""

   _agr_show_ghost() {
       local line="$READLINE_LINE"
       _agr_ghost_text=""

       if [[ "$line" =~ ^agr\ (play|analyze|optimize)\ $ ]]; then
           _agr_ghost_text=$(agr completions --files --limit 1 2>/dev/null)
           if [[ -n "$_agr_ghost_text" ]]; then
               # Save cursor, print ghost, restore cursor
               printf '\e[s\e[90;100m%s\e[0m\e[u' "$_agr_ghost_text"
           fi
       fi
   }

   _agr_accept_ghost() {
       if [[ -n "$_agr_ghost_text" ]]; then
           READLINE_LINE="${READLINE_LINE}${_agr_ghost_text}"
           READLINE_POINT=${#READLINE_LINE}
           _agr_ghost_text=""
           # Clear the ghost display
           printf '\e[K'
       fi
   }

   # Bind space to show ghost
   bind -x '"\x20":"_agr_show_ghost"'

   # Bind Tab to accept ghost (falls back to completion if no ghost)
   bind -x '"\C-i":"_agr_accept_ghost"'
   ```

3. **Handle partial input matching:**
   ```rust
   // In completions command handler, add --prefix filtering
   if args.files {
       let files = list_cast_files(&config)?;
       let filtered: Vec<_> = files
           .into_iter()
           .filter(|f| f.starts_with(&args.prefix))
           .take(args.limit)
           .collect();
       for f in filtered {
           println!("{}", f);
       }
       return Ok(());
   }
   ```

### Verification

```bash
# Install and test zsh
agr shell install
source ~/.zshrc
agr play <space>  # Should show ghost text

# Test acceptance
agr play <space><Tab>  # Should complete

# Test partial matching
agr play ses<Tab>  # Should complete session files starting with "ses"
```

### Files Changed
- `src/shell/completions.rs` -> updated with ghost text hooks

---

## Stage 5: Update `agr completions --files` for Ranked Output (REQ-5)

**Goal:** Return files sorted by modification time, most recent first.

### Tasks

1. **Update file listing in completions handler:**
   ```rust
   use std::cmp::Reverse;

   fn list_cast_files_ranked(config: &Config, limit: usize) -> Result<Vec<String>> {
       let recordings_dir = config.storage.recordings_dir.clone();
       let mut files: Vec<(PathBuf, SystemTime)> = Vec::new();

       // Walk all agent directories
       for entry in fs::read_dir(&recordings_dir)? {
           let entry = entry?;
           let agent_dir = entry.path();
           if agent_dir.is_dir() {
               for cast in fs::read_dir(&agent_dir)? {
                   let cast = cast?;
                   let path = cast.path();
                   if path.extension().map(|e| e == "cast").unwrap_or(false) {
                       if let Ok(meta) = path.metadata() {
                           if let Ok(mtime) = meta.modified() {
                               files.push((path, mtime));
                           }
                       }
                   }
               }
           }
       }

       // Sort by modification time, most recent first
       files.sort_by_key(|(_, mtime)| Reverse(*mtime));

       // Convert to short format (agent/filename.cast)
       let base = &recordings_dir;
       Ok(files
           .into_iter()
           .take(limit)
           .filter_map(|(path, _)| {
               path.strip_prefix(base)
                   .ok()
                   .map(|p| p.to_string_lossy().to_string())
           })
           .collect())
   }
   ```

2. **Add `--limit` flag handling:**
   ```rust
   // In completions.rs CLI definition (already added in Stage 3)
   #[arg(long, default_value = "10")]
   limit: usize,

   // Default 10 for ghost text, use higher for menu completion
   ```

3. **Optional: Add `--format=json` for richer metadata:**
   ```rust
   #[arg(long, value_enum, default_value = "plain")]
   format: OutputFormat,

   #[derive(clap::ValueEnum, Clone)]
   enum OutputFormat {
       Plain,
       Json,
   }

   // In handler:
   if args.format == OutputFormat::Json {
       let files: Vec<_> = list_cast_files_with_metadata(&config, args.limit)?;
       println!("{}", serde_json::to_string(&files)?);
   }
   ```

### Verification

```bash
# Test ranked output
agr completions --files --limit 5
# Should show 5 most recent files

# Test with prefix filter
agr completions --files --limit 10 claude/
# Should show only claude files, ranked by recency
```

### Files Changed
- `src/commands/completions.rs` -> updated

---

## Stage 6: Delete Old Files, Update Install Logic, Auto-Migrate

**Goal:** Clean up legacy completion files and ensure smooth upgrades.

### Tasks

1. **Delete static completion files:**
   ```bash
   git rm shell/completions.bash
   git rm shell/completions.zsh
   ```

2. **Remove embedded completion constants from `src/shell/completions.rs`:**
   ```rust
   // DELETE these lines:
   // pub const BASH_COMPLETIONS: &str = include_str!("../shell/completions.bash");
   // pub const ZSH_COMPLETIONS: &str = include_str!("../shell/completions.zsh");
   ```

3. **Update `install.rs` to use dynamic generation:**
   ```rust
   use super::completions::{generate_zsh_init, generate_bash_init};
   use super::minify::minify;

   pub fn generate_section(shell: Shell) -> String {
       let init_script = match shell {
           Shell::Zsh => generate_zsh_init(),
           Shell::Bash => generate_bash_init(),
       };

       let combined = format!("{}\n{}", SHELL_SCRIPT, init_script);
       let minified = minify(&combined);

       format!("{MARKER_START}\n{MARKER_WARNING}\n{minified}\n{MARKER_END}")
   }
   ```

4. **Add migration detection to install:**
   ```rust
   pub fn install(rc_file: &Path) -> io::Result<()> {
       let shell = detect_shell_from_rc(rc_file)?;

       // Check for existing installation (old or new)
       if is_installed_in(rc_file)? {
           // Auto-migrate: uninstall old, install new
           uninstall(rc_file)?;
       }

       // Generate new section with dynamic completions
       let section = generate_section(shell);

       // ... rest of install logic
   }

   fn detect_shell_from_rc(rc_file: &Path) -> io::Result<Shell> {
       let name = rc_file.file_name()
           .and_then(|n| n.to_str())
           .unwrap_or("");

       if name.contains("zsh") {
           Ok(Shell::Zsh)
       } else {
           Ok(Shell::Bash)
       }
   }
   ```

5. **Clean up old completion files on install:**
   ```rust
   pub fn cleanup_old_completions() -> io::Result<()> {
       // Remove old zsh completion file
       if let Some(path) = zsh_completion_path() {
           if path.exists() {
               fs::remove_file(&path)?;
           }
       }

       // Remove old bash completion file
       if let Some(path) = bash_completion_path() {
           if path.exists() {
               fs::remove_file(&path)?;
           }
       }

       Ok(())
   }
   ```

6. **Update shell command handler:**
   ```rust
   // In src/commands/shell.rs
   ShellCommands::Install => {
       // Clean up old completion files first
       shell::cleanup_old_completions()?;

       // Then install new integration
       shell::install(&rc_file)?;
   }
   ```

7. **Remove old completion installation from `agr.sh`:**
   - Remove `_agr_setup_completions()` function
   - Completions are now embedded in the generated section

### Verification

```bash
# Full migration test
# 1. Start with old installation
agr shell uninstall

# 2. Simulate old-style installation (manually add markers with old content)
# 3. Run install - should auto-migrate
agr shell install

# 4. Verify old files removed
ls ~/.zsh/completions/_agr  # Should not exist
ls ~/.local/share/bash-completion/completions/agr  # Should not exist

# 5. Verify new system works
source ~/.zshrc
agr <Tab>  # Should show all commands
agr play <Tab>  # Should show files
agr play <space>  # Should show ghost text
```

### Files Changed
- `shell/completions.bash` -> deleted
- `shell/completions.zsh` -> deleted
- `shell/agr.sh` -> updated (remove _agr_setup_completions)
- `src/shell/completions.rs` -> updated
- `src/shell/install.rs` -> updated
- `src/commands/shell.rs` -> updated

---

## Testing Requirements

### Unit Tests

| Module | Test Cases |
|--------|------------|
| `minify.rs` | Comments, blanks, shebang, inline hash |
| `completions.rs` | Command extraction, init generation |
| `install.rs` | Section generation, migration detection |
| `paths.rs` | RC detection, path resolution |

### Integration Tests

| Test | Description |
|------|-------------|
| Fresh install | Install on clean system |
| Migration | Upgrade from old installation |
| Uninstall | Complete removal |
| Completions | `agr <Tab>` works |
| Ghost text | Shows suggestion on space |

### Manual Testing Matrix

| Shell | Platform | Test |
|-------|----------|------|
| zsh | macOS | Full workflow |
| bash | macOS | Full workflow |
| zsh | Linux | Full workflow |
| bash | Linux | Full workflow |

---

## Rollback Plan

Each stage is independently reversible:

1. **Stage 1-2**: Revert to old `shell.rs` monolith
2. **Stage 3-4**: Disable `--shell-init`, restore static completion sourcing
3. **Stage 5**: Revert to unsorted file listing
4. **Stage 6**: Restore deleted completion files from git history

---

## Timeline Estimate

| Stage | Effort | Dependencies |
|-------|--------|--------------|
| 1. Refactor modules | 2-3h | None |
| 2. Minification | 1-2h | Stage 1 |
| 3. Shell init generation | 3-4h | Stage 1 |
| 4. Ghost text hooks | 4-5h | Stage 3 |
| 5. Ranked file output | 1-2h | None (parallel) |
| 6. Migration & cleanup | 2-3h | Stages 2-5 |

**Total: ~15-19 hours**

---

## Sign-off

- [ ] Architect: Plan reviewed and approved
- [ ] User: Timeline acceptable
