# Requirements: Fix Recording Rename Race Condition (File Loss Bug)

## Problem Statement

When a recording session runs for an extended period, external `agr` commands (e.g., `agr analyze`, `agr optimize`) can rename or transform the `.cast` file while asciinema is still recording. When the session ends, the post-recording rename prompt in `recording.rs` uses the original filepath captured at session start, which is now stale. The `std::fs::rename()` call fails with "No such file or directory (os error 2)" because the file was already moved.

**This caused actual data loss:** A 3-hour recording's `.cast` file was renamed by `agr analyze --suggest-rename` during the session. The post-session rename prompt then failed, and the user was left without a file at the expected path. Only the `.bak` backup (created by analyze) preserved the recording.

## Root Cause

`recording.rs:123` captures the filepath once at the start:
```rust
let filepath = agent_dir.join(&filename);
```

This filepath is used ~hours later at line 174 for `prompt_rename()`, but by then:
1. `agr analyze` may have created a `.bak` backup and added markers
2. `agr analyze --suggest-rename` or the auto-rename prompt may have renamed the file
3. `agr optimize` may have transformed the file in-place (preserving the name but creating `.bak`)

The rename at `recording.rs:219` then fails because the source file no longer exists at the stored path.

## User Stories

- As a user, I want my recording file to be protected from modification by other `agr` commands while it's being actively recorded, so that the post-session rename doesn't fail
- As a user, if my recording file was moved during a session, I want the rename prompt to find and use the actual current path, so that I don't lose my recording
- As a user, I want clear feedback if something went wrong with the file during recording, rather than a cryptic "No such file or directory" error
- As a user, I want to see in `agr list` which files are actively being recorded, so I don't accidentally modify them
- As a user, if I do select a locked file in `agr list`, I want the option to force-unlock it (e.g., if the recording crashed) and then proceed normally

## Acceptance Criteria

### Fix 1: Lock File During Active Recording (PRIMARY)

1. When `agr record` starts, a lock file MUST be created alongside the `.cast` file (e.g., `<filename>.cast.lock`)
2. The lock file MUST contain the recording PID for diagnostics
3. The lock file MUST be removed when the recording ends (both normal exit and interrupt/error)
4. Other `agr` commands that modify files (`analyze`, `optimize`, `marker add`) MUST check for a `.lock` file and refuse to operate on locked files with a clear message (e.g., "File is being recorded. Wait for the session to end.")
5. The `agr analyze` rename suggestion flow MUST also check for locks before renaming
6. Lock cleanup MUST be robust - stale locks from crashed processes should be detectable (via PID check)

### Fix 2: Inode-Based File Tracking Fallback (SECONDARY)

1. After asciinema creates the file, the recorder MUST capture the file's inode number (via `std::os::unix::fs::MetadataExt::ino()`)
2. Before the rename prompt, the recorder MUST verify the file still exists at the expected path
3. If the file is missing, the recorder MUST scan the agent directory for a file with the matching inode number
4. If a matching file is found at a different path, the rename prompt MUST use that path instead
5. If no matching file is found by inode (e.g., cross-filesystem move), fall back to header fingerprint matching (hash of the first JSON header line which contains the unique `timestamp` field)
6. If neither method finds the file, a clear error message MUST be shown explaining the file was moved externally, and the `.bak` file location SHOULD be suggested if one exists

### Fix 3: TUI Lock Awareness in `agr list`

1. In `agr list` / `agr ls`, files with an active `.lock` file MUST display a video camera icon (`📹`) to the right of the filename
2. Locked files MUST be visually greyed out in the file browser
3. When pressing Enter on a locked file, a dialog MUST appear explaining the file is locked and asking if the user wants to unlock it (force-remove the lock)
4. If the user confirms unlock, the lock file MUST be removed and the regular action dialog (play, analyze, etc.) MUST appear immediately
5. If the user declines unlock, the dialog MUST close and return to the file list with no action taken
6. Other keybindings on locked files (`c` copy, `d` delete, `e` explore, `a` analyze) SHOULD also show the lock dialog first before proceeding

### Fix 4: Graceful Error Recovery

1. If the rename fails for any reason, the original file MUST NOT be lost (it should remain at whatever path it currently occupies)
2. The error message MUST include the last known path and suggest checking `.bak` files
3. The post-session flow (auto-analyze, storage warning) MUST still execute even if rename fails

## Out of Scope

- Changing the analyze command's rename suggestion workflow
- Modifying the backup/restore system
- Adding file watching or inotify-based detection
- Multi-process locking (flock/advisory locks) - simple PID-based lock file is sufficient
- Changing how asciinema records files

## Constraints

- Lock files must use the same directory as the `.cast` file (no separate lock directory)
- Lock checking must be fast (stat + read, not scan) since it runs on every `agr` command
- Must maintain backward compatibility - old `agr` versions without lock awareness should still work (just won't check locks)
- No new external dependencies for locking
- Tests must be deterministic and not rely on timing

## Technical Notes

- **Inode tracking**: `std::os::unix::fs::MetadataExt::ino()` returns the filesystem inode, which survives renames on the same filesystem. This is the fastest and most reliable way to find a moved file. Scanning the agent directory by inode is O(n) where n is number of files in the directory.
- **Header fingerprint fallback**: The JSON header line (first line of `.cast` file) contains a unique `timestamp` field (Unix epoch of recording start). This survives marker additions, transforms, and even cross-filesystem moves. Used only if inode matching fails.
- Lock file format: plain text with PID on first line, filepath on second line
- Stale lock detection: check if PID is still running via `kill(pid, 0)` or `/proc/<pid>` on Linux / `kill -0` on macOS

Sign-off: Approved by user
