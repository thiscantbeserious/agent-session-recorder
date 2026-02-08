# ADR: Fix Recording Rename Race Condition

## Status
Accepted

## Context

During long recording sessions, other `agr` commands (`analyze`, `optimize`, `marker add`) can rename or transform the active `.cast` file. When the session ends, the post-recording rename prompt uses the original filepath captured at start, which is now stale. This caused actual data loss - a 3-hour recording was only recoverable from a `.bak` backup.

The codebase already has patterns for:
- Atomic file writes (temp+rename in `writer.rs`)
- Backup management (`files/backup.rs`)
- Disabled item guards in TUI context menu (Restore pattern in `list_app.rs`)
- Cached file metadata in `FileItem` (`has_backup` field)

### Prior Decisions
- Files use `.cast.bak` for backups, `.cast.tmp` for atomic writes
- TUI uses `Mode` enum for UI states, `ContextMenuItem` enum for actions
- `FileItem` caches metadata like `has_backup` for rendering

## Options Considered

### Option A: Lock File + Inode Tracking (Recommended)

**Prevention**: Create a `.cast.lock` file during recording that other commands check before modifying files.
**Recovery**: Track file by inode number. If the file moved, scan the agent directory by inode to find it.
**TUI**: Show locked files with `📹` icon, greyed out, with unlock dialog on interaction.

**Pros:**
- Lock prevents the problem entirely in the common case
- Inode tracking handles edge cases where lock was bypassed (old `agr` version, manual `agr analyze`)
- Lock files are simple, no external dependencies
- TUI integration follows established `has_backup` pattern
- PID-based stale lock detection handles crashes

**Cons:**
- Lock files can become stale if process crashes without cleanup
- Inode tracking is Unix-only (`MetadataExt::ino()`)
- Two mechanisms to maintain

### Option B: Lock File Only

Same as Option A but without inode/fingerprint fallback. If the lock is bypassed and the file moves, the rename fails.

**Pros:** Simpler implementation
**Cons:** No recovery path if lock is circumvented

### Option C: Inode Tracking Only (No Lock)

Track the file by inode at recording start, scan directory at rename time to find it wherever it moved.

**Pros:** No lock file management, no stale lock issues
**Cons:** Doesn't prevent the file from being modified (markers added, silence removed) during recording, which could cause other issues. Purely reactive, not preventive.

## Decision

**Option A: Lock File + Inode Tracking**

The lock file prevents the common case. Inode tracking provides a safety net for edge cases. Both are independently simple and compose well.

### Lock File Design

- **Path**: `<filename>.cast.lock` alongside the `.cast` file
- **Contents**: JSON `{"pid": <PID>, "started": "<ISO8601>"}`
- **Location**: New module `src/files/lock.rs` (~160-180 lines incl. tests)
- **Creation timing**: Create lock **before** asciinema starts recording (filepath is known at `recording.rs:123`). This protects the path from the moment recording begins, even though the `.cast` file doesn't exist yet. Commands check for `.lock` before operating on the `.cast`.
- **Stale detection**: Check if PID is alive via `kill(pid, 0)` / `std::process::Command` (already used in codebase)

### Lock Check Integration Points

Each command that modifies files checks for locks individually after file resolution:
- `src/commands/analyze.rs:71` - after `check_file_integrity()`, before analysis
- `src/commands/transform.rs:81` - after `check_file_integrity()`, before optimization
- `src/commands/marker.rs:20` - after resolution, before `handle_add()`
- `src/tui/list_app.rs:717` - in `optimize_session()` before `apply_transforms()` (bypasses CLI, needs own check)
- TUI context menu actions - gate via `lock_info` check before dispatching

**Note**: `resolve_file_path()` in `src/files/resolve.rs` stays pure (no lock check). Read-only commands (`play`, `list`, `copy`, `marker list`) should not be blocked by locks.

### Inode Fallback Design

- **Capture**: After `Command::new("asciinema").status()` returns (line 157+), the file exists on disk. Capture inode via `fs::metadata().ino()`.
- **Recovery**: In `prompt_rename()`, if file doesn't exist at expected path, call `find_by_inode(agent_dir, inode)`
- **Header fingerprint**: If inode fails (cross-fs move), hash first line of each `.cast` file in the directory
- **Location**: `src/files/lock.rs` for `find_by_inode()` and `find_by_header()`

### Graceful Rename Failure

**Critical fix**: `recording.rs:174` currently uses `?` which makes rename failure **fatal** - it skips `maybe_auto_analyze()` and `show_storage_warning()`. This is the same error pattern that caused the original data loss experience.

**Fix**: Change from `?` propagation to `match` with graceful fallback:
```rust
let final_filepath = match self.prompt_rename(&filepath, &filename, inode, &agent_dir) {
    Ok(path) => path,
    Err(e) => {
        eprintln!("  ⚠ Rename failed: {}", e);
        filepath.clone() // Continue with original path
    }
};
```

This ensures `maybe_auto_analyze` and `show_storage_warning` always execute.

### TUI Lock Awareness

- Add `lock_info: Option<LockInfo>` to `FileItem` (holds PID and start time for display)
- Render `📹` icon right of filename for locked files (`lock_info.is_some()`)
- Grey out locked files using `theme.text_secondary_style()`
- **Periodic refresh**: Every ~15s, re-check `read_lock()` for **visible** locked items only. When the lock disappears (recording ended), set to `None` - the `📹` icon disappears and the file becomes interactive automatically.
- On Enter/action on locked file: show dialog with lock details (PID, duration) and ask "Unlock?" (Yes/No)
- On Yes: remove lock file, refresh `lock_info` to `None`, proceed to regular action dialog
- On No: return to list

## Consequences

### Positive
- Recording files protected from concurrent modification
- Graceful recovery when protection is bypassed
- Rename failures no longer fatal (auto-analyze and storage warnings still run)
- TUI clearly communicates recording status with live refresh
- User can force-unlock stale locks from crashed sessions
- Follows established codebase patterns (`has_backup`, atomic writes)

### Negative
- Lock files add disk I/O on every command that modifies files (one `stat` call)
- Stale lock cleanup requires PID checking (platform-specific)
- `list_app.rs` (already 1351 lines) grows by ~75-110 lines - pre-existing tech debt, not introduced by this change

### Risks
- Low: Lock files are advisory - they don't prevent raw `mv` or non-agr tools from moving files. The inode fallback handles this.
- Low: Unix-only inode tracking. On non-Unix, falls back to header fingerprint matching.

## Testing Strategy

1. **Lock creation/removal**: Unit tests for create, check, remove, stale detection
2. **Lock enforcement**: Unit tests for commands refusing to operate on locked files
3. **Inode recovery**: Unit test creating a file, capturing inode, renaming file, finding by inode
4. **Header fingerprint**: Unit test matching files by header content
5. **TUI rendering**: Snapshot tests for locked file display
6. **Integration**: E2E test recording a file and verifying lock exists during, gone after
