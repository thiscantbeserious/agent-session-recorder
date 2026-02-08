# Review: Fix Recording Rename Race Condition - Phase internal

## Summary

Solid implementation of lock file protection + inode/header recovery that addresses the original data loss bug. The architecture follows established codebase patterns well and all tests pass cleanly. However, there are several issues: a lock file leak on early exit paths in recording, a performance concern with process spawning during FileItem construction, and `is_pid_alive` using an external process where a syscall would be safer and faster.

---

## Findings

### HIGH Severity

1. **src/recording.rs:126,159** - Lock file leaked if asciinema fails to start
   - Issue: `create_lock()` is called at line 126 with `?` propagation. If `Command::new("asciinema")...status()` fails at line 159 (also `?` propagation), the function returns early via `?` without ever reaching `lock::remove_lock(&filepath)` at line 165. This leaves an orphan lock file that will block all other commands from modifying the file until the stale PID detection kicks in. While stale detection will eventually clear it, the lock file references the *current process PID*, which will only appear stale once this process exits -- meaning immediate retries in the same process or rapid restarts could still see a "live" PID.
   - Impact: A user whose asciinema is misconfigured or whose agent binary does not exist will get a persistent lock file that blocks `agr analyze`, `agr optimize`, and `agr marker add` until the process exits and the stale detection clears it on next invocation. If the user retries recording in the same shell session immediately, the lock is for the current process PID which `is_pid_alive` will report as alive.
   - Fix: Use a guard pattern or `defer`-style cleanup. Either wrap the lock removal in a `Drop` guard struct, or change the `?` on the asciinema `.status()` call to a `match` that calls `remove_lock` before returning the error. For example:
     ```rust
     let status = match Command::new("asciinema")...status() {
         Ok(s) => s,
         Err(e) => {
             lock::remove_lock(&filepath);
             return Err(e).context("Failed to start asciinema");
         }
     };
     ```

### MEDIUM Severity

1. **src/files/lock.rs:125-132** - `is_pid_alive` uses process spawning instead of direct syscall
   - Issue: `is_pid_alive` spawns `kill -0 <pid>` as a child process. This has several problems: (a) It is slow -- spawning a process for each lock check. (b) The `kill` binary path is not validated and relies on `$PATH`. (c) On non-Unix platforms, there is no `kill` binary, so `is_pid_alive` will always return `false`, meaning *all* locks are treated as stale on Windows. This makes the lock mechanism effectively non-functional on Windows. (d) The function is `pub`, so any code can call it.
   - Impact: On Windows, all lock files are immediately cleaned up as stale, eliminating the protection entirely. The process-spawning cost is multiplied by the number of files during `FileItem` construction (see next finding).
   - Fix: Use `libc::kill(pid as i32, 0)` on Unix (unsafe but standard pattern, already common in Rust process management). For Windows, use `OpenProcess` / `GetExitCodeProcess` from the `windows` crate, or at minimum document that Windows is unsupported and gate the feature with `#[cfg(unix)]`.

2. **src/tui/widgets/file_explorer.rs:60,77** - `read_lock` called in FileItem constructor for every file on startup
   - Issue: `FileItem::new()` (line 60) and `From<SessionInfo>` (line 77) both call `lock::read_lock()` which internally calls `is_pid_alive()` which spawns a process. This happens for *every* session file when `agr list` starts. For a user with 50 recordings, this spawns 50 `kill` processes synchronously during startup.
   - Impact: Noticeable startup latency for `agr list` proportional to the number of session files. With 100+ files, this could add several hundred milliseconds to startup.
   - Fix: Either (a) lazily load `lock_info` only when a file is selected/visible, or (b) batch the lock check by first scanning for `.lock` files (a simple directory read) and only calling `is_pid_alive` for files that actually have a `.lock` file on disk. Most files will not have lock files, so this avoids the process-spawn cost entirely in the common case. Alternatively, restructure `read_lock` to check for lock file existence before parsing and checking PID liveness.

3. **src/recording.rs:164-169** - Inode and header captured after lock removal, creating a window where the file could be modified
   - Issue: The lock is removed at line 165, then inode is captured at line 168 and header at line 169. Between lock removal and inode/header capture, another `agr` command could rename the file. The inode capture would then fail (file no longer at `filepath`), and the header capture would also fail. The recovery mechanism then has no fallback data.
   - Impact: If another command is queued and executes in the brief window between lock removal and inode capture, recovery is degraded. This is a narrow race window and unlikely in practice, but the fix is simple.
   - Fix: Capture inode and header *before* removing the lock:
     ```rust
     let inode = Self::capture_inode(&filepath);
     let header = Self::read_header_line(&filepath);
     lock::remove_lock(&filepath);
     ```

### LOW Severity

1. **src/files/lock.rs:31-40** - `create_lock` does not check for existing lock files (no exclusivity)
   - Issue: `create_lock` calls `fs::write` which will silently overwrite an existing lock file. If two `agr record` processes start simultaneously for the same file, the second will overwrite the first's lock. The first process will then have no protection.
   - Impact: Very unlikely edge case (two recordings to the same file), but violates the lock contract. The second recorder overwrites the lock with its own PID, so when the first finishes and calls `remove_lock`, it removes the second's lock.
   - Fix: Use `OpenOptions::new().create_new(true).write(true)` to fail if the lock already exists, then check for stale PID before overwriting.

2. **src/files/lock.rs:125** - `is_pid_alive` is `pub` but should be internal
   - Issue: `is_pid_alive` is declared `pub` but is only used internally by `read_lock`. Exposing it as a public API invites misuse and makes it part of the module's contract.
   - Impact: API surface is wider than necessary.
   - Fix: Change to `pub(crate)` or `fn` (private).

3. **src/tui/list_app.rs:386-413** - Inconsistent lock gating on TUI shortcuts: `p` (play) is gated but play is a read-only operation
   - Issue: The `p` shortcut for playing a session shows the `ConfirmUnlock` dialog even though playing is a read-only operation. The ADR explicitly states that read-only commands (`play`, `list`, `copy`, `marker list`) should not be blocked by locks. However, `play` and `copy` (`c`) are both gated with `is_selected_locked()` checks (lines 387 and 394).
   - Impact: Users are forced to go through an unlock dialog to play or copy a file that is being recorded, which contradicts the stated design principle and the behavior of the CLI commands (where `play` is not blocked).
   - Fix: Remove the lock gate from `play_session` and `copy_to_clipboard` shortcuts, matching the CLI behavior where read-only commands are not blocked.

4. **tests/integration/lock_test.rs** - Tests for `read_lock_returns_none_for_stale_pid` use PID 999999999 which is not guaranteed to be dead
   - Issue: PID 999999999 is used as a "definitely dead" process. While practically always dead, on systems with PID namespace wrapping or containerized environments, this is not guaranteed. The test could flake.
   - Impact: Potential flaky test in unusual environments.
   - Fix: Use a more robust approach: fork a child process, wait for it to exit, then use its PID.

---

## Test Quality

The test suite is solid with 14 lock-specific tests and 4 lock enforcement tests in `transform_test.rs`. Coverage is good for the happy path and several edge cases:

**Strengths:**
- Round-trip test for lock create/read
- Stale PID detection tested
- Auto-cleanup of stale locks verified
- Inode recovery after rename
- Header fingerprint matching with positive and negative cases
- Non-cast file exclusion for both inode and header scanners
- Lock enforcement tests for transform, analyze, and marker commands
- Stale lock auto-clean in transform command

**Gaps:**
- No test for lock file leak on early error exit (the HIGH finding)
- No test for concurrent lock creation (two processes writing the same lock)
- No test for `resolve_actual_path` recovery chain (tested only via individual finders, not the composed fallback logic)
- The `ConfirmUnlock` mode transition is tested only via primitive lock operations, not via simulated key events
- Snapshot tests for TUI lock rendering are deferred (noted in PLAN as incomplete)

---

## ADR/PLAN Compliance

The implementation closely follows the ADR and PLAN. All four stages are marked complete with appropriate checkmarks. Key deviations noted:

1. **Snapshot tests deferred** - PLAN Stage 4 marks snapshot tests as "deferred - structural tests sufficient". This is a conscious tradeoff documented in the plan. Acceptable for internal review.

2. **Read-only commands gated in TUI** - The ADR states "Read-only commands (play, list, copy, marker list) should not be blocked by locks." The CLI implementation correctly follows this (no lock check in `play` or `copy` commands), but the TUI implementation gates `play` and `copy` behind the unlock dialog. This deviates from the ADR.

3. **Lock file format** - REQUIREMENTS.md says "Lock file format: plain text with PID on first line, filepath on second line" but the implementation uses JSON `{"pid":..., "started":...}`. The ADR specifies JSON, so this follows the ADR rather than the REQUIREMENTS doc. The ADR takes precedence.

---

## Verdict

**REQUEST_CHANGES**

The HIGH finding (lock leak on asciinema failure) is a real bug that will leave orphan lock files in a common failure scenario (misconfigured asciinema, missing agent binary). The MEDIUM findings around read-only operation gating in the TUI and the inode capture ordering are worth addressing before the PR is marked ready. The `is_pid_alive` process-spawning approach works but has known platform limitations that should at minimum be documented.

The overall design is sound and the implementation quality is good. These are fixable issues, not architectural problems.
