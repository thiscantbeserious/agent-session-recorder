# ADR: Clipboard Copy Feature

## Status
Proposed

## Context

Users want to share `.cast` recording files on Slack and similar platforms. Currently, there's no way to copy a recording file to the clipboard from within the tool. Users must manually navigate to the recordings directory and use OS-level file operations.

### Requirements Summary

1. CLI command `agr copy <recording>` to copy recordings to clipboard
2. TUI integration with context menu option and `c` keybinding
3. Cross-platform support: macOS and Linux only (Windows explicitly out of scope)
4. Graceful fallback to content copy when file copy isn't supported
5. TDD methodology throughout implementation

### Technical Context

The codebase already has:
- `resolve_file_path()` - path resolution supporting absolute, short format, and fuzzy matching
- `ContextMenuItem` enum in `src/tui/list_app.rs` with established patterns
- Shell-out pattern using `std::process::Command` (e.g., asciinema integration)
- No existing clipboard dependencies

## Decision

### Approach: Shell-out to OS Tools

Shell out to native OS clipboard tools at runtime. No Rust clipboard crates - they don't support file-to-clipboard (only text/images), which is the primary use case for Slack sharing.

**macOS tools:**
- `osascript` - AppleScript for file copy (primary)
- `pbcopy` - content fallback

**Linux tools (tried in order):**
- `xclip` - X11 clipboard with file URI support
- `xsel` - X11 clipboard alternative
- `wl-copy` - Wayland clipboard

Tools are tried in order and allowed to fail naturally. No explicit X11 vs Wayland detection.

**Availability detection (hybrid approach):**
- **macOS tools**: Compile-time `cfg!(target_os = "macos")` — osascript/pbcopy are always present on macOS
- **Linux tools**: Runtime `tool_exists()` check — xclip/xsel/wl-copy may not be installed

### Keybinding: `c` for Copy

Natural and intuitive ("c for copy"), consistent with desktop conventions.

---

## Module Architecture

### Directory Structure

```
src/
├── clipboard/
│   ├── mod.rs              # Public API: copy_file_to_clipboard()
│   ├── result.rs           # CopyResult, CopyMethod enums
│   ├── error.rs            # ClipboardError enum
│   ├── copy.rs             # Copy struct - orchestrates tool execution
│   ├── tool.rs             # CopyTool trait definition
│   └── tools/
│       ├── mod.rs          # Tool exports + platform_tools() function
│       ├── osascript.rs    # OsaScript - macOS file copy
│       ├── pbcopy.rs       # Pbcopy - macOS content copy
│       ├── xclip.rs        # Xclip - Linux X11
│       ├── xsel.rs         # Xsel - Linux X11 alternative
│       └── wl_copy.rs      # WlCopy - Linux Wayland
├── commands/
│   └── copy.rs             # CLI handler
└── tui/
    └── list_app.rs         # TUI integration (modified)
```

**Rationale**: Standalone module follows the existing domain-focused pattern. Each module owns one capability (`shell/` = shell integration, `clipboard/` = clipboard integration).

### Public Interface

```rust
// src/clipboard/mod.rs

pub use result::{CopyResult, CopyMethod};
pub use error::ClipboardError;

/// Copy a file to the system clipboard.
///
/// Tries to copy the file as a file reference first (for paste-as-file in Slack, etc.).
/// Falls back to copying the file's text content if file copy isn't supported.
///
/// # Errors
/// - `ClipboardError::FileNotFound` - file doesn't exist
/// - `ClipboardError::NoToolAvailable` - no clipboard tool found
/// - `ClipboardError::ToolFailed` - clipboard operation failed
pub fn copy_file_to_clipboard(path: &Path) -> Result<CopyResult, ClipboardError> {
    Copy::new().file(path)
}
```

### Core Types

```rust
// src/clipboard/result.rs

/// The result of a clipboard copy operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    /// File was copied as a file reference (can paste as file attachment)
    FileCopied { tool: CopyMethod },
    /// File content was copied as text (fallback when file copy unavailable)
    ContentCopied { tool: CopyMethod, size_bytes: usize },
}

impl CopyResult {
    /// Create a FileCopied result
    pub fn file_copied(tool: CopyMethod) -> Self {
        Self::FileCopied { tool }
    }

    /// Create a ContentCopied result
    pub fn content_copied(tool: CopyMethod, size_bytes: usize) -> Self {
        Self::ContentCopied { tool, size_bytes }
    }

    /// User-friendly message describing what happened
    pub fn message(&self, filename: &str) -> String {
        match self {
            Self::FileCopied { .. } => {
                format!("Copied {}.cast to clipboard", filename)
            }
            Self::ContentCopied { .. } => {
                format!(
                    "Copied {}.cast content to clipboard (file copy not supported on this platform)",
                    filename
                )
            }
        }
    }

    /// Whether this was a true file copy (not content fallback)
    pub fn is_file_copy(&self) -> bool {
        matches!(self, Self::FileCopied { .. })
    }
}

/// Which tool was used for the copy operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMethod {
    OsaScript,  // macOS AppleScript
    Pbcopy,     // macOS pasteboard
    Xclip,      // Linux X11
    Xsel,       // Linux X11 alternative
    WlCopy,     // Linux Wayland
}

impl CopyMethod {
    /// Tool name for display/logging
    pub fn name(&self) -> &'static str {
        match self {
            Self::OsaScript => "osascript",
            Self::Pbcopy => "pbcopy",
            Self::Xclip => "xclip",
            Self::Xsel => "xsel",
            Self::WlCopy => "wl-copy",
        }
    }
}
```

```rust
// src/clipboard/error.rs

/// Errors that can occur during clipboard operations.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("No clipboard tool available. On Linux, install xclip, xsel, or wl-copy.")]
    NoToolAvailable,

    #[error("Clipboard tool '{tool}' failed: {message}")]
    ToolFailed { tool: &'static str, message: String },

    #[error("Failed to read file: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Platform not supported (only macOS and Linux)")]
    UnsupportedPlatform,
}
```

### CopyTool Trait

```rust
// src/clipboard/tool.rs

/// A tool that can copy content to the system clipboard.
///
/// Each implementation wraps a specific OS tool (osascript, xclip, etc.)
/// and knows how to invoke it correctly.
pub trait CopyTool: Send + Sync {
    /// The method identifier for this tool
    fn method(&self) -> CopyMethod;

    /// Human-readable name for error messages
    fn name(&self) -> &'static str {
        self.method().name()
    }

    /// Check if this tool is available on the system.
    ///
    /// Should be fast - typically checks if the binary exists.
    fn is_available(&self) -> bool;

    /// Whether this tool supports copying files as file references.
    ///
    /// If false, only `try_copy_text` will be called.
    fn can_copy_files(&self) -> bool;

    /// Try to copy a file as a file reference.
    ///
    /// The file at `path` should be copyable to apps that accept file drops.
    fn try_copy_file(&self, path: &Path) -> Result<(), CopyToolError>;

    /// Try to copy text content to the clipboard.
    fn try_copy_text(&self, text: &str) -> Result<(), CopyToolError>;
}

/// Error from a specific tool operation.
#[derive(Debug, Clone)]
pub enum CopyToolError {
    /// Tool doesn't support this operation
    NotSupported,
    /// Tool execution failed
    Failed(String),
    /// Tool not found on system
    NotFound,
}
```

### Copy Orchestrator

```rust
// src/clipboard/copy.rs

/// Orchestrates clipboard copy operations using available tools.
///
/// Tries tools in priority order:
/// 1. File copy with tools that support it
/// 2. Content copy as fallback
pub struct Copy {
    tools: Vec<Box<dyn CopyTool>>,
}

impl Copy {
    /// Create with platform-appropriate tools.
    pub fn new() -> Self {
        Self {
            tools: tools::platform_tools(),
        }
    }

    /// Create with specific tools (for testing).
    pub fn with_tools(tools: Vec<Box<dyn CopyTool>>) -> Self {
        Self { tools }
    }

    /// Copy a file to the clipboard.
    ///
    /// Tries file copy first, falls back to content copy.
    pub fn file(&self, path: &Path) -> Result<CopyResult, ClipboardError> {
        // Validate file exists
        if !path.exists() {
            return Err(ClipboardError::FileNotFound {
                path: path.to_path_buf(),
            });
        }

        // Try file copy with tools that support it
        for tool in &self.tools {
            if tool.is_available() && tool.can_copy_files() {
                match tool.try_copy_file(path) {
                    Ok(()) => {
                        return Ok(CopyResult::file_copied(tool.method()));
                    }
                    Err(CopyToolError::NotSupported) => continue,
                    Err(CopyToolError::NotFound) => continue,
                    Err(CopyToolError::Failed(_)) => continue, // Try next tool
                }
            }
        }

        // Fall back to content copy
        let content = std::fs::read_to_string(path)?;
        let size = content.len();

        for tool in &self.tools {
            if tool.is_available() {
                match tool.try_copy_text(&content) {
                    Ok(()) => {
                        return Ok(CopyResult::content_copied(tool.method(), size));
                    }
                    Err(CopyToolError::NotSupported) => continue,
                    Err(CopyToolError::NotFound) => continue,
                    Err(CopyToolError::Failed(_)) => continue,
                }
            }
        }

        Err(ClipboardError::NoToolAvailable)
    }
}

impl Default for Copy {
    fn default() -> Self {
        Self::new()
    }
}
```

### Tool Implementations (Example: OsaScript)

```rust
// src/clipboard/tools/osascript.rs

/// macOS AppleScript clipboard tool.
///
/// Uses `osascript` to copy files as POSIX file references.
/// This allows pasting as actual file attachments in Slack, etc.
pub struct OsaScript;

impl OsaScript {
    pub fn new() -> Self {
        Self
    }

    /// Escape a path for use in AppleScript string.
    fn escape_path(path: &Path) -> String {
        path.display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    }

    /// Build the AppleScript command for file copy.
    fn build_file_script(path: &Path) -> String {
        format!(
            "set the clipboard to POSIX file \"{}\"",
            Self::escape_path(path)
        )
    }

    /// Run an AppleScript.
    fn run_script(script: &str) -> Result<(), CopyToolError> {
        let output = Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| CopyToolError::Failed(e.to_string()))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(CopyToolError::Failed(
                String::from_utf8_lossy(&output.stderr).to_string()
            ))
        }
    }
}

impl CopyTool for OsaScript {
    fn method(&self) -> CopyMethod {
        CopyMethod::OsaScript
    }

    fn is_available(&self) -> bool {
        cfg!(target_os = "macos")
    }

    fn can_copy_files(&self) -> bool {
        true
    }

    fn try_copy_file(&self, path: &Path) -> Result<(), CopyToolError> {
        let script = Self::build_file_script(path);
        Self::run_script(&script)
    }

    fn try_copy_text(&self, _text: &str) -> Result<(), CopyToolError> {
        // osascript can do text, but pbcopy is simpler/faster
        Err(CopyToolError::NotSupported)
    }
}

impl Default for OsaScript {
    fn default() -> Self {
        Self::new()
    }
}
```

### Platform Tool Selection

```rust
// src/clipboard/tools/mod.rs

mod osascript;
mod pbcopy;
mod xclip;
mod xsel;
mod wl_copy;

pub use osascript::OsaScript;
pub use pbcopy::Pbcopy;
pub use xclip::Xclip;
pub use xsel::Xsel;
pub use wl_copy::WlCopy;

use super::tool::CopyTool;

/// Get the platform-appropriate tools in priority order.
pub fn platform_tools() -> Vec<Box<dyn CopyTool>> {
    #[cfg(target_os = "macos")]
    {
        vec![
            Box::new(OsaScript::new()),
            Box::new(Pbcopy::new()),
        ]
    }

    #[cfg(target_os = "linux")]
    {
        vec![
            Box::new(Xclip::new()),
            Box::new(Xsel::new()),
            Box::new(WlCopy::new()),
        ]
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        vec![]
    }
}

/// Check if a command-line tool exists.
pub(crate) fn tool_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

---

## TUI Integration

### Context Menu Changes

```rust
// src/tui/list_app.rs - ContextMenuItem enum

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuItem {
    Play,
    Copy,        // NEW - position after Play
    Optimize,
    Restore,
    Delete,
    AddMarker,
}

impl ContextMenuItem {
    pub const ALL: [ContextMenuItem; 6] = [  // Updated count
        ContextMenuItem::Play,
        ContextMenuItem::Copy,      // NEW
        ContextMenuItem::Optimize,
        ContextMenuItem::Restore,
        ContextMenuItem::Delete,
        ContextMenuItem::AddMarker,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            ContextMenuItem::Play => "Play",
            ContextMenuItem::Copy => "Copy to clipboard",  // NEW
            ContextMenuItem::Optimize => "Optimize",
            ContextMenuItem::Restore => "Restore from backup",
            ContextMenuItem::Delete => "Delete",
            ContextMenuItem::AddMarker => "Add marker",
        }
    }

    pub fn shortcut(&self) -> &'static str {
        match self {
            ContextMenuItem::Play => "p",
            ContextMenuItem::Copy => "c",  // NEW
            ContextMenuItem::Optimize => "t",
            ContextMenuItem::Restore => "r",
            ContextMenuItem::Delete => "d",
            ContextMenuItem::AddMarker => "m",
        }
    }
}
```

---

## CLI Integration

```rust
// src/cli.rs - Commands enum addition

/// Copy a recording to the system clipboard
#[command(long_about = "Copy a recording file to the system clipboard.

Copies the .cast file so it can be pasted into Slack, email, or other apps
that accept file attachments. On platforms where file copy isn't supported,
falls back to copying the file's JSON content as text.

EXAMPLES:
    agr copy session.cast                 Copy by filename (fuzzy match)
    agr copy claude/session.cast          Copy using short format
    agr copy /path/to/session.cast        Copy by absolute path

SUPPORTED PLATFORMS:
    macOS    File copy via osascript (paste as file attachment)
    Linux    File copy via xclip, or content copy via xsel/wl-copy")]
Copy {
    /// Path to the .cast file to copy
    #[arg(help = "Path to the .cast recording file")]
    file: String,  // Named "file" to enable shell completion auto-detection
},
```

---

## Shell Completion Integration

The existing shell completion system automatically detects commands that accept file arguments by checking for a positional argument with `id == "file"`. By naming our argument `file` (not `recording`), the `copy` command automatically gets recording completion support.

### How It Works

1. **Detection**: `src/shell/completions.rs` has `has_file_argument()` that checks:
   ```rust
   fn has_file_argument(cmd: &clap::Command) -> bool {
       cmd.get_positionals().any(|arg| arg.get_id() == "file")
   }
   ```

2. **Generation**: Commands with `file` arguments are added to `_agr_file_cmds` in the generated shell init code.

3. **Completion**: When the user types `agr copy <TAB>`, the shell calls:
   ```bash
   agr completions --files --limit 20 "$prefix"
   ```

4. **Results**: `StorageManager::list_cast_files_short()` returns recordings in `agent/filename.cast` format.

### Verification

After implementation, verify completions work:

```bash
# Regenerate shell init (if using shell integration)
agr shell install

# Or test directly
agr completions --files ""           # Should list all recordings
agr completions --files "claude/"    # Should list claude recordings

# Test in shell
agr copy <TAB>                       # Should show recording completions
```

### No Code Changes Required

Because we name the argument `file`, the existing completion infrastructure handles everything automatically:
- `extract_commands()` will include `copy` in its output
- `has_file_argument()` will return `true` for `copy`
- Shell init code will include `copy` in `_agr_file_cmds`
- Tab completion will work for `agr copy <TAB>`

---

## Consequences

### What becomes easier
- Quick sharing of recordings to Slack/Teams/etc.
- Single-action copy from CLI or TUI
- No need to navigate to storage directory manually

### What becomes harder
- Nothing significant

### Technical debt considerations
- Platform-specific code requires testing on multiple OSes
- Tool availability varies across Linux distributions

### Out of scope (documented for future)
- Windows support
- Batch copy of multiple recordings
- Copy file path instead of file

---

## Implementation Stages

See `PLAN.md` for detailed checklists. High-level flow:

```
Stage 1 (Types) ──> Stage 2 (Orchestrator) ──┬──> Stage 3 (macOS Tools)
                                              └──> Stage 4 (Linux Tools)
                                                        │
                                              ┌─────────┴─────────┐
                                              v                   v
                                       Stage 5 (API) ─────────────┤
                                              │                   │
                              ┌───────────────┴───────────────┐   │
                              v                               v   │
                       Stage 6 (CLI)                   Stage 7 (TUI)
                              │                               │
                              └───────────┬───────────────────┘
                                          v
                                   Stage 8 (Documentation)
                                          │
                                          v
                                   Stage 9 (Integration Tests)
                                          │
                                          v
                                   Stage 10 (Manual Testing)
```

| Stage | Focus | Files |
|-------|-------|-------|
| 1 | Core types: result, error, tool trait | `clipboard/{result,error,tool}.rs` |
| 2 | Copy orchestrator with MockTool tests | `clipboard/copy.rs` |
| 3 | macOS tools: OsaScript + Pbcopy | `clipboard/tools/{osascript,pbcopy}.rs` |
| 4 | Linux tools: Xclip + Xsel + WlCopy | `clipboard/tools/{xclip,xsel,wl_copy}.rs` |
| 5 | Platform selection + public API | `clipboard/{tools/mod,mod}.rs` |
| 6 | CLI: definition + handler + dispatch | `cli.rs`, `commands/copy.rs`, `main.rs` |
| 7 | TUI: menu + action + help | `tui/list_app.rs` |
| 8 | Documentation | `README.md`, `docs/` |
| 9 | Integration tests + completions | `tests/integration/copy_test.rs` |
| 10 | Manual platform testing | N/A |

**Parallelization:** Stages 3+4 can run in parallel. Stages 6+7 can run in parallel after Stage 5.

---

## Decision History

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Implementation approach | Shell-out to OS tools | Rust clipboard crates don't support file-to-clipboard |
| Keybinding | `c` | Natural "copy" convention |
| Linux tool order | xclip -> xsel -> wl-copy | xclip most common, wl-copy for Wayland |
| Wayland handling | Try tools in order | No explicit X11/Wayland detection |
| Naming scheme | Action-centric | Natural reading: `Copy::new().file(path)` |
| Module structure | Trait-based with tools/ | Clean separation, excellent testability |
| CLI argument name | `file` (not `recording`) | Enables automatic shell completion via existing detection |
| Stage consolidation | 10 stages (from 21) | Reduced bloat while preserving TDD detail |
