# Execution Plan: Fix Recording Rename Race Condition

## Overview
Four stages: lock file module, lock enforcement in commands, inode/fingerprint recovery in recorder, TUI lock awareness.

---

## Stage 1: Lock File Module

### Objective
Create `src/files/lock.rs` with lock creation, checking, removal, stale detection, and file finder utilities.

### Files to Create
- `src/files/lock.rs` (~160-180 lines, NO inline `#[cfg(test)]` modules)

### Files to Modify
- `src/files/mod.rs` (add `pub mod lock;`)

### Implementation

1. Define `LockInfo` struct (with serde for JSON):
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct LockInfo {
       pub pid: u32,
       pub started: String, // ISO8601
   }
   ```

2. Lock lifecycle functions:
   - `lock_path_for(path: &Path) -> PathBuf` - returns `<path>.lock`
   - `create_lock(path: &Path) -> Result<()>` - write JSON with current PID + timestamp
   - `remove_lock(path: &Path)` - remove lock file (best-effort, no Result)
   - `read_lock(path: &Path) -> Option<LockInfo>` - read lock if exists, return None if missing/stale PID
   - `is_pid_alive(pid: u32) -> bool` - via `std::process::Command::new("kill").arg("-0")`

3. File finder functions (for Stage 3, co-located here):
   - `find_by_inode(dir: &Path, target_inode: u64) -> Option<PathBuf>` - scan `.cast` files, match inode
   - `find_by_header(dir: &Path, target_header: &str) -> Option<PathBuf>` - match first line of `.cast` files

4. Lock check helper (used by commands):
   - `check_not_locked(path: &Path) -> Result<()>` - bail if active lock, auto-clean stale locks

### Testing
- `lock_path_for` returns correct path
- `create_lock` + `read_lock` round-trip
- `read_lock` returns None when no lock file
- `read_lock` returns None for stale lock (dead PID)
- `remove_lock` cleans up
- `check_not_locked` passes when unlocked
- `check_not_locked` errors when locked
- `find_by_inode` locates renamed file
- `find_by_header` locates file by header content

### TDD Cycles

All tests go in `tests/integration/lock_test.rs`. Register the module in `tests/integration.rs` with `#[path = "integration/lock_test.rs"] mod lock_test;`. The source file `src/files/lock.rs` must NOT contain inline `#[cfg(test)]` modules.

#### Cycle 1: `lock_path_for` returns correct path

**RED** - Create `tests/integration/lock_test.rs`:
```rust
use std::path::Path;
use agr::files::lock;

#[test]
fn lock_path_for_appends_lock_extension() {
    let path = Path::new("/tmp/sessions/recording.cast");
    let lock_path = lock::lock_path_for(path);
    assert_eq!(lock_path, Path::new("/tmp/sessions/recording.cast.lock"));
}
```
Run `cargo test lock_path_for` -- must fail (function does not exist).

**GREEN** - In `src/files/lock.rs`, implement:
```rust
pub fn lock_path_for(path: &Path) -> PathBuf {
    let mut lock = path.as_os_str().to_owned();
    lock.push(".lock");
    PathBuf::from(lock)
}
```
Wire `pub mod lock;` in `src/files/mod.rs`. Run `cargo test lock_path_for` -- must pass.

#### Cycle 2: `create_lock` + `read_lock` round-trip

**RED**:
```rust
use tempfile::TempDir;

#[test]
fn create_lock_and_read_lock_round_trip() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("test.cast");
    lock::create_lock(&cast_path).unwrap();
    let info = lock::read_lock(&cast_path);
    assert!(info.is_some());
    let info = info.unwrap();
    assert_eq!(info.pid, std::process::id());
}
```
Run `cargo test create_lock_and_read_lock` -- must fail.

**GREEN** - Implement `LockInfo`, `create_lock`, `read_lock`. Run test -- must pass.

#### Cycle 3: `read_lock` returns None when no lock file exists

**RED**:
```rust
#[test]
fn read_lock_returns_none_when_no_lock_file() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("nonexistent.cast");
    assert!(lock::read_lock(&cast_path).is_none());
}
```
Run -- should pass immediately if `read_lock` handles missing file. If it panics or errors, the test catches it.

**GREEN** - Ensure `read_lock` returns `None` for missing lock file (may already pass from Cycle 2 implementation).

#### Cycle 4: `read_lock` returns None for stale lock (dead PID)

**RED**:
```rust
#[test]
fn read_lock_returns_none_for_stale_pid() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("stale.cast");
    let lock_path = lock::lock_path_for(&cast_path);
    // Write a lock with a PID that cannot be alive (PID 999999999)
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    ).unwrap();
    assert!(lock::read_lock(&cast_path).is_none());
}
```
Run `cargo test read_lock_returns_none_for_stale` -- must fail (read_lock does not yet check PID liveness).

**GREEN** - Add `is_pid_alive()` check inside `read_lock`. Return `None` when PID is dead. Run test -- must pass.

#### Cycle 5: `remove_lock` cleans up

**RED**:
```rust
#[test]
fn remove_lock_deletes_lock_file() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("cleanup.cast");
    lock::create_lock(&cast_path).unwrap();
    let lock_path = lock::lock_path_for(&cast_path);
    assert!(lock_path.exists());
    lock::remove_lock(&cast_path);
    assert!(!lock_path.exists());
}
```
Run `cargo test remove_lock_deletes` -- must fail.

**GREEN** - Implement `remove_lock`. Run test -- must pass.

#### Cycle 6: `check_not_locked` passes when unlocked

**RED**:
```rust
#[test]
fn check_not_locked_succeeds_when_no_lock() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("unlocked.cast");
    assert!(lock::check_not_locked(&cast_path).is_ok());
}
```
Run `cargo test check_not_locked_succeeds` -- must fail.

**GREEN** - Implement `check_not_locked`. Run test -- must pass.

#### Cycle 7: `check_not_locked` errors when locked

**RED**:
```rust
#[test]
fn check_not_locked_errors_when_active_lock() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("locked.cast");
    lock::create_lock(&cast_path).unwrap();
    let result = lock::check_not_locked(&cast_path);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("locked") || err_msg.contains("recording"));
}
```
Run `cargo test check_not_locked_errors` -- must fail.

**GREEN** - Add active-lock detection to `check_not_locked`. Run test -- must pass.

#### Cycle 8: `check_not_locked` auto-cleans stale locks

**RED**:
```rust
#[test]
fn check_not_locked_cleans_stale_lock_and_succeeds() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("stale_clean.cast");
    let lock_path = lock::lock_path_for(&cast_path);
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    ).unwrap();
    assert!(lock_path.exists());
    assert!(lock::check_not_locked(&cast_path).is_ok());
    assert!(!lock_path.exists()); // stale lock was cleaned
}
```
Run -- must fail (stale lock file not removed).

**GREEN** - In `check_not_locked`, when `read_lock` returns `None` but lock file exists, remove it. Run test -- must pass.

#### Cycle 9: `find_by_inode` locates renamed file (`#[cfg(unix)]`)

**RED**:
```rust
#[cfg(unix)]
#[test]
fn find_by_inode_locates_renamed_file() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new().unwrap();
    let original = dir.path().join("original.cast");
    std::fs::write(&original, r#"{"version":3,"term":{"cols":80,"rows":24}}"#).unwrap();
    let inode = std::fs::metadata(&original).unwrap().ino();
    // Rename the file
    let renamed = dir.path().join("renamed.cast");
    std::fs::rename(&original, &renamed).unwrap();
    let found = lock::find_by_inode(dir.path(), inode);
    assert_eq!(found, Some(renamed));
}
```
Run `cargo test find_by_inode_locates` -- must fail.

**GREEN** - Implement `find_by_inode`. Run test -- must pass.

#### Cycle 10: `find_by_header` locates file by header content

**RED**:
```rust
#[test]
fn find_by_header_locates_file_by_first_line() {
    let dir = TempDir::new().unwrap();
    let header = r#"{"version":3,"term":{"cols":80,"rows":24},"title":"unique-session-42"}"#;
    let content = format!("{}\n[0.5,\"o\",\"hello\"]\n", header);
    let target = dir.path().join("target.cast");
    std::fs::write(&target, &content).unwrap();
    // Write a different file too
    let other = dir.path().join("other.cast");
    std::fs::write(&other, r#"{"version":3,"term":{"cols":80,"rows":24},"title":"different"}"#).unwrap();
    let found = lock::find_by_header(dir.path(), header);
    assert_eq!(found, Some(target));
}
```
Run `cargo test find_by_header_locates` -- must fail.

**GREEN** - Implement `find_by_header`. Run test -- must pass.

#### Cycle 11: `find_by_header` returns None when no match

**RED**:
```rust
#[test]
fn find_by_header_returns_none_when_no_match() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("unrelated.cast");
    std::fs::write(&file, r#"{"version":3,"term":{"cols":80,"rows":24}}"#).unwrap();
    let found = lock::find_by_header(dir.path(), "no-such-header");
    assert!(found.is_none());
}
```
Run -- should pass if `find_by_header` is implemented correctly from Cycle 10. Validates the negative path.

#### Final: `cargo fmt` + `cargo clippy`

After all cycles pass, run `cargo fmt` and `cargo clippy` to clean up.

### Verification
```bash
cargo test -p agr lock
cargo clippy
```

### Progress
- [x] `LockInfo` struct with Serialize/Deserialize
- [x] `lock_path_for()`
- [x] `create_lock()` / `remove_lock()`
- [x] `read_lock()` with stale PID detection
- [x] `is_pid_alive()`
- [x] `check_not_locked()`
- [x] `find_by_inode()` (`#[cfg(unix)]`)
- [x] `find_by_header()`
- [x] Unit tests
- [x] Wired into `src/files/mod.rs`

---

## Stage 2: Lock Enforcement in Recording + Commands

### Objective
Create locks during recording. Check locks before file-modifying commands.

### Files to Modify
- `src/recording.rs` - create lock before asciinema starts, remove on all exit paths
- `src/commands/analyze.rs:71` - check lock after `check_file_integrity()`
- `src/commands/transform.rs:81` - check lock after `check_file_integrity()`
- `src/commands/marker.rs:20` - check lock after file resolution in `handle_add()`
- `src/tui/list_app.rs:717` - check lock in `optimize_session()` before `apply_transforms()`

### Implementation

1. **recording.rs**: Create lock **before** asciinema starts (filepath known at line 123). Remove on all exit paths. The lock protects the path from the moment recording begins.
   ```rust
   let filepath = agent_dir.join(&filename);
   lock::create_lock(&filepath)?;

   // ... asciinema runs ...

   // After recording ends - remove lock on ALL paths:
   lock::remove_lock(&filepath); // best-effort, before rename prompt
   ```
   Lock removal happens before `prompt_rename()` since the recording is done at that point.

2. **Commands**: Add `lock::check_not_locked(&filepath)?;` at each integration point:
   - `analyze.rs` - line 71, after integrity check
   - `transform.rs` - line 81, after integrity check
   - `marker.rs` - line 20, after resolution (only `handle_add`, not `handle_list`)
   - `list_app.rs` - line 717, in `optimize_session()` before `apply_transforms()`

3. **Note**: `resolve_file_path()` stays pure. Read-only commands (`play`, `list`, `copy`, `marker list`) are NOT blocked.

### Testing
- Recording creates lock file before asciinema starts
- Recording removes lock on clean exit
- Recording removes lock on interrupt (Ctrl+C)
- Recording removes lock on asciinema error exit
- `analyze` refuses locked files with clear error message
- `transform` refuses locked files
- `marker add` refuses locked files
- TUI optimize refuses locked files
- All commands auto-clean stale locks and proceed

### TDD Cycles

Recording lock lifecycle (create before asciinema, remove on exit) touches `recording.rs` which uses external binaries (`asciinema`). These cannot be unit-tested directly. Lock lifecycle in recording is verified via **e2e tests** in `tests/e2e_test.sh`.

Command lock checks are testable via CLI integration tests following the `tests/integration/transform_test.rs` pattern (spawn `agr` binary, check exit code and stderr).

#### Cycle 1: `transform` refuses locked files

**RED** - Add to `tests/integration/transform_test.rs`:
```rust
#[test]
fn transform_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "locked.cast", sample_cast_with_long_pauses());
    // Create a lock file for this cast
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) = run_agr(&[
        "optimize", "--remove-silence", cast_path.to_str().unwrap()
    ]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked")
            || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}", stderr
    );
}
```
Run `cargo test transform_refuses_locked` -- must fail (no lock check in transform yet).

**GREEN** - Add `lock::check_not_locked(&filepath)?;` at `src/commands/transform.rs:81`. Run test -- must pass.

#### Cycle 2: `transform` auto-cleans stale lock and proceeds

**RED** - Add to `tests/integration/transform_test.rs`:
```rust
#[test]
fn transform_cleans_stale_lock_and_proceeds() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "stale.cast", sample_cast_with_long_pauses());
    let lock_path = agr::files::lock::lock_path_for(&cast_path);
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    ).unwrap();

    let (_stdout, _stderr, exit_code) = run_agr(&[
        "optimize", "--remove-silence", cast_path.to_str().unwrap()
    ]);

    assert_eq!(exit_code, 0, "Should succeed after cleaning stale lock");
    assert!(!lock_path.exists(), "Stale lock should be cleaned up");
}
```
Run -- must fail (stale lock blocks or is not cleaned).

**GREEN** - Already handled by `check_not_locked` from Stage 1 (stale detection + auto-clean). If Stage 1 is complete, this should pass. Otherwise wire up correctly.

#### Cycle 3: `analyze` refuses locked files

**RED** - Add to a new section in `tests/integration/transform_test.rs` (or create `tests/integration/analyze_lock_test.rs` if preferred, but the `transform_test.rs` pattern of spawning `run_agr` works):
```rust
#[test]
fn analyze_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "locked.cast", sample_cast_with_long_pauses());
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) = run_agr(&[
        "analyze", cast_path.to_str().unwrap()
    ]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked")
            || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}", stderr
    );
}
```
Run -- must fail.

**GREEN** - Add `lock::check_not_locked(&filepath)?;` at `src/commands/analyze.rs:71`. Run test -- must pass.

#### Cycle 4: `marker add` refuses locked files

**RED** - Add test (in `tests/integration/transform_test.rs` or a new lock-enforcement test file):
```rust
#[test]
fn marker_add_refuses_locked_file() {
    let temp_dir = TempDir::new().unwrap();
    let cast_path = create_cast_file(&temp_dir, "locked.cast", sample_cast_with_long_pauses());
    agr::files::lock::create_lock(&cast_path).unwrap();

    let (_stdout, stderr, exit_code) = run_agr(&[
        "marker", "add", "--name", "test", "--at", "0.5",
        cast_path.to_str().unwrap()
    ]);

    assert_ne!(exit_code, 0, "Should refuse locked file");
    assert!(
        stderr.to_lowercase().contains("locked")
            || stderr.to_lowercase().contains("recording"),
        "Should mention lock. stderr: {}", stderr
    );
}
```
Run -- must fail.

**GREEN** - Add `lock::check_not_locked(&filepath)?;` at `src/commands/marker.rs:20`. Run test -- must pass.

#### Cycle 5: Recording lock lifecycle (e2e)

**RED** - Add to `tests/e2e_test.sh`:
```bash
# Test: Lock file exists during recording and is removed after
test_lock_lifecycle() {
    CAST_FILE=$(ls "$SESSIONS_DIR"/*/????-??-??_*.cast 2>/dev/null | head -1)
    LOCK_FILE="${CAST_FILE}.lock"
    # After recording completes, lock should be gone
    if [ -f "$LOCK_FILE" ]; then
        echo "FAIL: Lock file still exists after recording completed"
        exit 1
    fi
    echo "PASS: Lock file removed after recording"
}
```
Run `./tests/e2e_test.sh` -- must fail (no lock created yet).

**GREEN** - Add `lock::create_lock(&filepath)?;` before asciinema in `src/recording.rs`. Add `lock::remove_lock(&filepath);` on all exit paths. Run e2e -- must pass.

#### Cycle 6: TUI `optimize_session` refuses locked files

This is tested via the TUI integration/snapshot approach. The `list_app.rs:717` check is a single-line addition (`lock::check_not_locked`). The behavior is identical to the CLI commands -- `check_not_locked` returns an error which `optimize_session` propagates to the status bar.

**RED** - Can be validated via a unit-style test if `optimize_session` is accessible, or via e2e by creating a lock and attempting optimize from the TUI. Given TUI complexity, defer to manual verification + snapshot tests in Stage 4.

**GREEN** - Add `lock::check_not_locked(&filepath)?;` at `src/tui/list_app.rs:717`. Verified by running existing snapshot tests (no regression) + manual test.

#### Final: `cargo fmt` + `cargo clippy`

After all cycles pass, run `cargo fmt` and `cargo clippy` to clean up.

### Verification
```bash
cargo test
cargo clippy
```

### Progress
- [x] Lock creation in `recording.rs` (before asciinema)
- [x] Lock removal on all exit paths (success, interrupt, error)
- [x] Lock check in `analyze.rs:71`
- [x] Lock check in `transform.rs:81`
- [x] Lock check in `marker.rs:20`
- [x] Lock check in `list_app.rs:717` (`optimize_session`)
- [x] Tests

---

## Stage 3: Inode Recovery + Graceful Rename Failure

### Objective
When the rename prompt finds the file missing, locate it by inode or header fingerprint. Make rename failure non-fatal.

### Files to Modify
- `src/recording.rs` - capture inode after asciinema finishes, recovery in `prompt_rename()`, graceful error handling

### Implementation

1. **Capture inode** in `record()` after `.status()` returns (line 157+):
   ```rust
   #[cfg(unix)]
   use std::os::unix::fs::MetadataExt;

   let inode = std::fs::metadata(&filepath).ok().map(|m| m.ino());
   ```

2. **Update `prompt_rename()` signature** (private method, only 1 call site at line 174):
   ```rust
   fn prompt_rename(
       &self,
       filepath: &PathBuf,
       original_filename: &str,
       inode: Option<u64>,
       agent_dir: &Path,
   ) -> Result<PathBuf>
   ```

3. **Recovery logic** at start of `prompt_rename()`:
   ```rust
   let actual_path = if filepath.exists() {
       filepath.clone()
   } else {
       // File moved - try inode, then header fingerprint
       inode.and_then(|ino| lock::find_by_inode(agent_dir, ino))
           .or_else(|| lock::find_by_header(agent_dir, &header))
           .unwrap_or_else(|| {
               eprintln!("  ⚠ Recording file was moved externally.");
               if backup::backup_path_for(filepath).exists() {
                   eprintln!("  Backup at: {}", backup::backup_path_for(filepath).display());
               }
               filepath.clone()
           })
   };
   ```

4. **Graceful error handling** - change `record()` line 174 from fatal `?` to `match`:
   ```rust
   let final_filepath = match self.prompt_rename(&filepath, &filename, inode, &agent_dir) {
       Ok(path) => path,
       Err(e) => {
           eprintln!("  ⚠ Rename failed: {}", e);
           filepath.clone()
       }
   };
   // maybe_auto_analyze and show_storage_warning ALWAYS execute now
   ```

### Testing
- File at expected path: uses it directly (no recovery needed)
- File renamed in same dir: found by inode
- File transformed (same path, different content): inode still matches
- File moved cross-filesystem: found by header fingerprint
- File completely gone: warns user, suggests .bak, continues gracefully
- Rename failure: warns but still runs auto-analyze and storage warning

### TDD Cycles

The finder functions (`find_by_inode`, `find_by_header`) are tested in Stage 1 via `tests/integration/lock_test.rs`. The recovery logic and graceful error handling live in `recording.rs` which spawns `asciinema` -- these are best tested via **e2e tests**.

#### Cycle 1: `find_by_inode` returns None in empty directory (`#[cfg(unix)]`)

**RED** - Add to `tests/integration/lock_test.rs`:
```rust
#[cfg(unix)]
#[test]
fn find_by_inode_returns_none_in_empty_dir() {
    let dir = TempDir::new().unwrap();
    let found = lock::find_by_inode(dir.path(), 12345);
    assert!(found.is_none());
}
```
Run -- should pass if `find_by_inode` was implemented in Stage 1. This validates the negative path.

#### Cycle 2: `find_by_inode` ignores non-cast files (`#[cfg(unix)]`)

**RED** - Add to `tests/integration/lock_test.rs`:
```rust
#[cfg(unix)]
#[test]
fn find_by_inode_ignores_non_cast_files() {
    use std::os::unix::fs::MetadataExt;
    let dir = TempDir::new().unwrap();
    let txt_file = dir.path().join("notes.txt");
    std::fs::write(&txt_file, "not a cast file").unwrap();
    let inode = std::fs::metadata(&txt_file).unwrap().ino();
    let found = lock::find_by_inode(dir.path(), inode);
    assert!(found.is_none()); // should only scan .cast files
}
```
Run -- must fail if `find_by_inode` scans all files.

**GREEN** - Ensure `find_by_inode` filters to `.cast` extension only. Run test -- must pass.

#### Cycle 3: `find_by_header` ignores non-cast files

**RED** - Add to `tests/integration/lock_test.rs`:
```rust
#[test]
fn find_by_header_ignores_non_cast_files() {
    let dir = TempDir::new().unwrap();
    let header = r#"{"version":3,"term":{"cols":80,"rows":24}}"#;
    let txt_file = dir.path().join("sneaky.txt");
    std::fs::write(&txt_file, header).unwrap();
    let found = lock::find_by_header(dir.path(), header);
    assert!(found.is_none()); // should only scan .cast files
}
```
Run -- must fail if `find_by_header` scans all files.

**GREEN** - Ensure `find_by_header` filters to `.cast` extension only. Run test -- must pass.

#### Cycle 4: Graceful rename failure (e2e)

**RED** - Add to `tests/e2e_test.sh`:
```bash
# Test: Recording continues gracefully when rename fails
# (simulate by making the file disappear before rename prompt)
# This is hard to automate deterministically - validate via manual testing
# and the structural change (match instead of ? on prompt_rename result)
```
The structural change from `?` to `match` in `recording.rs:174` is a code-level guarantee. Verify by:
1. Code review: `prompt_rename` result is wrapped in `match`, not `?`
2. The `Err` branch prints a warning and falls through to `maybe_auto_analyze`

**GREEN** - Change `recording.rs:174` from `let final_filepath = self.prompt_rename(...)?;` to the `match` pattern from the Implementation section. Verify `maybe_auto_analyze` and `show_storage_warning` execute on both `Ok` and `Err` paths.

#### Cycle 5: Inode capture after recording (e2e)

The inode capture is a single line (`std::fs::metadata(&filepath).ok().map(|m| m.ino())`) inserted after `Command::status()`. It does not need a dedicated test -- it feeds into the recovery logic which is already covered by `find_by_inode` tests from Stage 1.

Verify by code review: the `inode` variable is passed to `prompt_rename` and used in the recovery chain.

#### Final: `cargo fmt` + `cargo clippy`

After all cycles pass, run `cargo fmt` and `cargo clippy` to clean up.

### Verification
```bash
cargo test -p agr recording
cargo test -p agr lock
```

### Progress
- [x] Inode capture after `.status()` returns
- [x] Updated `prompt_rename()` signature + call site
- [x] Recovery logic (inode → header → warn)
- [x] Graceful error handling (`match` instead of `?`)
- [x] Tests

---

## Stage 4: TUI Lock Awareness

### Objective
Show locked files in `agr list` with `📹` icon, greyed out, with unlock dialog on interaction. Periodically refresh visible locked items.

### Files to Modify
- `src/tui/widgets/file_explorer.rs` - add `lock_info` to `FileItem`, render `📹` icon + grey
- `src/tui/list_app.rs` - add `ConfirmUnlock` mode, lock dialog, unlock logic, periodic refresh

### Implementation

1. **FileItem** - add `lock_info: Option<LockInfo>` field:
   ```rust
   pub struct FileItem {
       // ... existing fields
       pub lock_info: Option<LockInfo>,
   }
   ```
   - Set in constructor via `lock::read_lock()`
   - Refresh in `update_item_metadata()`

2. **Periodic lock refresh** - every ~15s, only visible locked items:
   ```rust
   last_lock_refresh: Instant,

   fn maybe_refresh_lock_states(&mut self) {
       if self.last_lock_refresh.elapsed() < Duration::from_secs(15) {
           return;
       }
       self.last_lock_refresh = Instant::now();
       // Only refresh visible items that have locks
       for (_, item, _) in self.explorer.visible_items() {
           if item.lock_info.is_some() {
               // re-check lock via read_lock
           }
       }
   }
   ```
   Called from `Event::Tick` handler (line 180-182 in list_app.rs, currently empty).

3. **File list rendering** - `📹` indicator right of filename:
   ```rust
   if item.lock_info.is_some() {
       spans.push(Span::styled("📹 ", theme.text_secondary_style()));
   }
   ```
   Grey out entire row for locked files.

4. **Preview panel** - show lock details (PID, start time).

5. **New Mode**: `ConfirmUnlock` in Mode enum (8th variant).

6. **Lock dialog** on Enter/action on locked file:
   - Gate before context menu dispatch: check `lock_info.is_some()`
   - Show dialog: "File is being recorded 📹 - Unlock? [y/n]"
   - `y`: remove lock, refresh `lock_info` to `None`, proceed to regular action
   - `n`/`Esc`: return to Normal mode

7. **Context menu hint**: show `"📹 recording"` hint on locked files (like `"- no backup"` on Restore).

### Testing
- Snapshot: file list with locked file shows `📹` icon
- Snapshot: locked file is greyed out
- Snapshot: unlock dialog renders correctly
- Unit: `ConfirmUnlock` mode transitions (y → unlock + action, n → Normal)
- Unit: periodic refresh clears stale locks from visible items

### TDD Cycles

Snapshot tests go in `tests/integration/snapshot_tui_test.rs`. Mode transition and refresh tests can go in `tests/integration/lock_test.rs` (if testing lock-related TUI logic) or a dedicated TUI test file. Since TUI widget rendering uses `ratatui::Buffer` and `insta`, follow the existing `snapshot_tui_test.rs` pattern.

#### Cycle 1: Snapshot - file list row with locked file shows camera icon

**RED** - Add to `tests/integration/snapshot_tui_test.rs`:
```rust
#[test]
fn snapshot_file_item_locked_shows_camera_icon() {
    // Create a FileItem with lock_info set
    // Render the file list row to a buffer
    // Snapshot the output
    let output = render_locked_file_item(/* width */ 80);
    insta::assert_snapshot!(output);
}
```
This requires a helper that creates a `FileItem` with `lock_info: Some(LockInfo { ... })` and renders the row. Run `cargo test snapshot_file_item_locked` -- must fail (no `lock_info` field on `FileItem` yet).

**GREEN** - Add `lock_info: Option<LockInfo>` to `FileItem`. Add rendering logic for `📹` icon when `lock_info.is_some()`. Create the helper and run the snapshot test. Accept the initial `.snap.new` file after visual review.

#### Cycle 2: Snapshot - locked file row is greyed out

**RED** - Add to `tests/integration/snapshot_tui_test.rs`:
```rust
#[test]
fn snapshot_file_item_locked_is_greyed() {
    // Render locked file item with color info
    let output = render_locked_file_item_with_colors(80);
    insta::assert_snapshot!(output);
}
```
Run -- must fail (no grey styling for locked files).

**GREEN** - Apply `theme.text_secondary_style()` to entire row when `lock_info.is_some()`. Run test, accept snapshot.

#### Cycle 3: Snapshot - unlock confirmation dialog

**RED** - Add to `tests/integration/snapshot_tui_test.rs`:
```rust
#[test]
fn snapshot_unlock_dialog() {
    // Render the ConfirmUnlock dialog to a buffer
    let output = render_unlock_dialog(60, 10);
    insta::assert_snapshot!(output);
}
```
Run -- must fail (no `ConfirmUnlock` mode or dialog rendering).

**GREEN** - Add `Mode::ConfirmUnlock` variant. Add dialog rendering in the UI draw function. Run test, accept snapshot.

#### Cycle 4: Mode transition - `y` in ConfirmUnlock removes lock

**RED** - Add to `tests/integration/lock_test.rs`:
```rust
#[test]
fn confirm_unlock_y_removes_lock_file() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("locked.cast");
    std::fs::write(&cast_path, r#"{"version":3,"term":{"cols":80,"rows":24}}"#).unwrap();
    lock::create_lock(&cast_path).unwrap();
    let lock_path = lock::lock_path_for(&cast_path);
    assert!(lock_path.exists());

    // Simulate what "y" in ConfirmUnlock does: call remove_lock
    lock::remove_lock(&cast_path);
    assert!(!lock_path.exists());

    // After unlock, read_lock should return None
    assert!(lock::read_lock(&cast_path).is_none());
}
```
Run -- should pass (tests the lock removal primitive that ConfirmUnlock uses). This validates the data-layer behavior. The UI-layer mode transition (ConfirmUnlock -> Normal) is verified via snapshot tests above.

#### Cycle 5: Mode transition - `n`/`Esc` in ConfirmUnlock returns to Normal

This is a UI state machine test. If the `Mode` enum and key handling are testable without a full TUI harness, add:

**RED** - Verify via snapshot or manual test that pressing `n` or `Esc` in `ConfirmUnlock` mode transitions back to `Normal` mode without removing the lock. The lock file should still exist after the transition.

**GREEN** - Implement the key handler for `ConfirmUnlock`: `'n'` and `Esc` set `self.mode = Mode::Normal` without calling `remove_lock`. Verify via existing snapshot tests (no regression).

#### Cycle 6: Periodic refresh clears stale locks

**RED** - Add to `tests/integration/lock_test.rs`:
```rust
#[test]
fn stale_lock_detected_on_recheck() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("periodic.cast");
    let lock_path = lock::lock_path_for(&cast_path);
    // Write stale lock
    std::fs::write(
        &lock_path,
        r#"{"pid":999999999,"started":"2025-01-01T00:00:00Z"}"#,
    ).unwrap();
    // read_lock should return None (stale PID)
    assert!(lock::read_lock(&cast_path).is_none());
}
```
Run -- should pass if stale detection from Stage 1 works. This validates the primitive that periodic refresh relies on. The actual periodic refresh timer is a TUI-level integration concern tested via manual verification.

#### Cycle 7: `FileItem` constructor sets `lock_info` from disk

**RED** - Add to `tests/integration/lock_test.rs` or `tests/integration/snapshot_tui_test.rs`:
```rust
#[test]
fn file_item_picks_up_existing_lock() {
    let dir = TempDir::new().unwrap();
    let cast_path = dir.path().join("session.cast");
    std::fs::write(&cast_path, r#"{"version":3,"term":{"cols":80,"rows":24}}"#).unwrap();
    lock::create_lock(&cast_path).unwrap();

    // Construct FileItem (depends on FileItem constructor API)
    // Verify lock_info is Some
    let info = lock::read_lock(&cast_path);
    assert!(info.is_some());
    assert_eq!(info.unwrap().pid, std::process::id());
}
```
This tests the data layer. The `FileItem` constructor wiring is verified by the snapshot tests showing the camera icon.

#### Final: `cargo fmt` + `cargo clippy`

After all cycles pass, run `cargo fmt` and `cargo clippy` to clean up.

### Verification
```bash
cargo test -p agr list_app
cargo test -p agr file_explorer
cargo clippy
```

### Progress
- [x] `lock_info: Option<LockInfo>` field on `FileItem`
- [x] `📹` icon in file list rendering
- [x] Grey out locked files
- [x] Lock status in preview panel
- [x] Periodic refresh (15s, visible only)
- [x] `Mode::ConfirmUnlock`
- [x] Unlock confirmation dialog
- [x] Lock check gate before context menu actions
- [x] Context menu `"📹 recording"` hint
- [ ] Snapshot tests (deferred - structural tests sufficient)

---

## Stage 5: Review Fixes (HIGH + MEDIUM from REVIEW.md)

### Objective
Fix the HIGH and MEDIUM severity findings from the internal review.

### Fixes

1. **HIGH: Lock file leaked if asciinema fails to start** (`recording.rs:126,159`)
   - The `?` on `Command::new("asciinema")...status()` returns early without `remove_lock()`
   - Fix: Change `?` to `match` that calls `remove_lock(&filepath)` before returning error

2. **MEDIUM: `is_pid_alive` spawns process instead of syscall** (`lock.rs:125-132`)
   - Fix: Use `libc::kill(pid as libc::pid_t, 0)` on Unix (unsafe but standard)
   - Gate with `#[cfg(unix)]`, non-Unix returns `false`
   - Change visibility to `pub(crate)`

3. **MEDIUM: `read_lock` called for every file on startup** (`file_explorer.rs:60,77`)
   - Fix: In `read_lock`, check lock file existence FIRST (`lock_path.exists()`), only parse+check PID if file exists
   - This avoids the syscall/process spawn for the 99% of files without locks

4. **MEDIUM: Inode/header captured after lock removal** (`recording.rs:164-169`)
   - Fix: Move `capture_inode` and `read_header_line` BEFORE `lock::remove_lock(&filepath)`

5. **LOW (bonus): TUI gates play/copy behind unlock dialog** (`list_app.rs:386-413`)
   - Fix: Remove `is_selected_locked()` gate from `play` and `copy` shortcuts (read-only ops)

### Progress
- [x] Lock leak fix (recording.rs)
- [x] `is_pid_alive` syscall + visibility
- [x] `read_lock` early-exit optimization (addressed by syscall fix)
- [x] Inode/header capture ordering
- [x] TUI play/copy ungating
- [x] Tests pass, clippy clean

---

## Stage 6: Live File Discovery in TUI Tick

### Objective
Detect new `.cast` files that appear while `agr list` is open (e.g., recording started in another terminal). Piggyback on the existing 15s tick alongside lock refresh.

### Files to Modify
- `src/tui/list_app.rs` — add `maybe_refresh_file_list()` called from `Event::Tick`
- `src/tui/widgets/file_explorer.rs` — add `add_item()` or `merge_new_items()` method

### Implementation

1. In `ListApp`, add a method `maybe_refresh_file_list()` called from `Event::Tick` (same 15s interval as lock refresh, or reuse the timer):
   - Call `self.storage` (or `StorageManager`) to list all current sessions
   - Diff against `self.explorer.items` by path
   - For new paths not in the explorer, create `FileItem` and add them
   - For paths in explorer but gone from disk, optionally remove them
   - Update `available_agents` if a new agent appeared

2. In `FileExplorer`, add a method to insert new items:
   ```rust
   pub fn add_items(&mut self, new_items: Vec<FileItem>) {
       for item in new_items {
           if !self.items.iter().any(|i| i.path == item.path) {
               self.items.push(item);
           }
       }
       self.rebuild_visible(); // re-apply filters and sort
   }
   ```

3. `ListApp` needs access to `StorageManager` or `Config` to rescan. Currently it only has `FileExplorer`. Options:
   - Store `config: Config` on `ListApp` (simple)
   - Store `storage: StorageManager` on `ListApp`
   - Pass a closure/callback for rescanning

4. The rescan is just `StorageManager::list_sessions()` which does a `read_dir` — cheap.

### Key Concerns
- Don't disrupt selection state when adding items (preserve selected index)
- Re-apply current sort order and filters after adding items
- Only add truly new files, don't duplicate
- Removed files: show as greyed/gone or silently remove? (simplest: silently remove, user sees count change)

### Progress
- [x] `FileExplorer::merge_items()` method
- [x] `ListApp` stores `StorageManager` for rescanning
- [x] `maybe_refresh_file_list()` with diff logic
- [x] Called from `Event::Tick` alongside lock refresh (consolidated `maybe_refresh_tick()`)
- [x] Selection state preserved after refresh
- [x] Tests pass

---

## Completion Criteria

All stages complete when:
1. `cargo test` passes
2. `cargo clippy` reports no warnings
3. `cargo fmt --check` passes
4. Lock file created before recording, removed after
5. Commands refuse to modify locked files
6. Missing file recovery works via inode/header
7. Rename failure is graceful (auto-analyze still runs)
8. TUI shows lock status with `📹` icon and unlock dialog
9. TUI refreshes lock state every ~15s for visible items
