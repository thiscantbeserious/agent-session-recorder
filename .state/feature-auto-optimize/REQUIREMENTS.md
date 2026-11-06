# Requirements: Auto-Optimize After Recording

Branch: feature/auto-optimize (from main 9ffd6f9). Must merge BEFORE the open sync PR #166 (feature/sync-gist), so sync's auto push runs after the final recording flow: record, optimize, compress, (later) push.

## Problem Statement

Long agent sessions contain lots of idle time (agent thinking, waiting on tools). Today `agr optimize --remove-silence` removes that, but only by hand, one file at a time, so almost nobody does it and recordings stay long, large and tedious to play back. Sync (#166) will also upload whatever is on disk at the end of the recording flow, so optimizing must happen before that point and not as an afterthought.

## Desired Outcome

After implementation:

1. A finished recording can be silence-optimized automatically, as part of the recording flow, without running `agr optimize` manually.
2. The behaviour is controlled by one config key under `[recording]`, `auto_optimize`, default `false`, next to `auto_analyze`.
3. The recording flow order is fixed: record, lock removed, optimize, compress, stop banner, auto-analyze, storage warning. (Sync push comes last, later.)
4. A failure in auto-optimize never loses or damages the recording.
5. The existing manual `agr optimize` keeps working unchanged.

## Fixed Decisions (made by the user, 2026-11-06, not up for discussion)

1. **Default:** off, opt-in. `[recording] auto_optimize = false`, like `auto_analyze`. (2026-11-06)
2. **Order:** the plain `.cast` is optimized BEFORE compression, so there is no decompress/recompress round trip. Flow: record, lock removed, optimize, compress, stop banner, auto-analyze, storage warning. (2026-11-06)
3. **Backup:** a hidden `.bak` of the unoptimized recording is KEPT, exactly like manual `agr optimize` (the backup the existing `apply_transforms` creates). No skip switch. (2026-11-06)
4. **Threshold:** header `idle_time_limit`, else 2.0 s, same as manual optimize. No new config value. (2026-11-06)
5. **Failure:** keep the unoptimized file, print one warning line, exit status unchanged. (2026-11-06)
6. **Analyze order:** optimize runs before auto-analyze so markers and timestamps match the optimized file. (2026-11-06)
7. **Bulk optimize** of old recordings is out of scope. (2026-11-06)

## Scope

### In Scope

**Recording flow**
- After the recording ends and the lock file is removed, and before `finalize_recording` (gzip), the plain `.cast` is silence-optimized in place using the existing `apply_transforms` path (src/asciicast/transform_ops.rs). No decompress/recompress round trip.
- Only silence removal, threshold resolved as in manual optimize (header `idle_time_limit`, else 2.0 s).
- Live or locked recordings are never touched. Auto-optimize only runs on the file of the recording that just finished, after its own lock is gone.
- Optimize runs before `maybe_auto_analyze`.

**Backup**
- The hidden `.bak` of the unoptimized recording is created by the existing `apply_transforms` behaviour, no new backup code.
- When compression then runs, the backup must end up consistent with the session format (`.<name>.cast.gz.bak`, the existing hidden convention). To be verified, not assumed.

**Config**
- New key `auto_optimize` (boolean, default `false`) in `[recording]`, added through a versioned config migration (v5 on this branch, see constraints).
- Documented in `src/config/docs.rs` and the README.

**Failure handling**
- If optimize fails (parse error, write error), the unoptimized file stays as is, exactly one warning line is printed, the rest of the flow (compress, banner, analyze, warning) continues, and the exit status of the recording is unchanged.

### Out of Scope

- Other transforms beyond silence removal.
- Sync, push, Gists (#166).
- Bulk optimize of existing recordings (many files in one command).
- Changes to `agr optimize` CLI behaviour or flags.
- A config value for the threshold, or a switch to skip the backup.
- Changes to compression, analyze or storage-warning logic other than their position in the flow.
- Optimizing while recording.

## Acceptance Criteria

**Flow and config**
- [ ] With `auto_optimize = true`, a finished recording with idle gaps above the threshold has those gaps reduced, and the result is stored in the configured format (`.cast.gz` under default compression, `.cast` under `"none"`).
- [ ] With `auto_optimize = true` and compression gzip, the file is optimized while still plain and compressed afterwards, with no gzip file being decoded and re-encoded in the flow.
- [ ] With the default config (`auto_optimize = false`), the finished recording is byte-identical to today's behaviour and no `.bak` is created.
- [ ] The threshold is the header `idle_time_limit` if present, else 2.0 s.
- [ ] While recording, the file is plain and locked, and nothing optimizes it. Optimize starts only after the lock file is removed.
- [ ] A session with a live lock is never modified by auto-optimize.
- [ ] `agr analyze` (auto_analyze) runs after optimize, and the markers it writes refer to the optimized timestamps.
- [ ] Order of output and steps is: optimize, compress, stop banner, auto-analyze, storage warning.

**Backup**
- [ ] With `auto_optimize = true`, a hidden `.bak` of the unoptimized recording exists after the flow, created like manual `agr optimize` does.
- [ ] After compression, the backup follows the session format: for `<name>.cast.gz` it is `.<name>.cast.gz.bak`, with no leftover `.<name>.cast.bak`, and it is a valid backup of the unoptimized content (restorable, content matches the pre-optimize recording after decoding).
- [ ] With `compression = "none"`, the backup is `.<name>.cast.bak`.
- [ ] The `.bak` is never listed as a session and is not counted as one in status, list or cleanup, and cleanup removes it together with its session.

**Failure**
- [ ] If optimize fails, the unoptimized recording is intact and still gets compressed per config, one warning line is printed, and the recording exit status is unchanged.
- [ ] A recording without any gap above the threshold is left functionally unchanged and does not produce a warning.

**Config**
- [ ] A config without the new key is migrated to the next version and loads with `auto_optimize = false`. Running the migration twice changes nothing.
- [ ] An invalid value gives a clear error.

**Regression**
- [ ] `agr optimize --remove-silence` on `.cast` and `.cast.gz` behaves as before.
- [ ] `cargo test`, `cargo clippy` and `cargo fmt --check` pass.
- [ ] README and `src/config/docs.rs` describe the new key and the flow order.

**Default and post-migrate prompt** (user decision 2026-11-06, replaces the opt-in default)
- [ ] `[recording] auto_optimize` is a setting whose default answer is yes.
- [ ] After `agr config migrate` adds the key, agr asks "Optimize recordings automatically after each session? [Y/n]"; Enter keeps the default yes, "n" writes false.
- [ ] Without a TTY or with `agr config migrate --yes` the default (yes) is written without asking.
- [ ] The question is asked only once: never again once the key exists in the config.
- [ ] The post-migrate question is a general mechanism (a registry of post-migrate prompts with key, question, default, condition and how the answer is written), auto-optimize is its first entry, later features (sync auto push, compression offer) can add entries without changing the runner.

## Constraints

- Config changes go through the versioned migrations. Current version on main is 4, the sync branch (#166) adds v5. This feature is meant to merge first, so it takes v5 and sync renumbers to v6. If sync lands first instead, this branch renumbers to v6. Whoever merges second renumbers.
- Live and locked recordings must never be touched.
- No in-place unsafe rewrite: reuse the existing atomic write of the transform path.
- The hidden `.bak` from optimize follows the PR #160 hidden-file convention and is never a session.
- Keep the diff narrow: reuse `apply_transforms`, do not add new transform code.
- Branch merges before #166 and must not depend on it.

## Context

- Verified on main: `agr optimize --remove-silence[=<secs>] [--output <file>] <file>` (src/commands/transform.rs, src/asciicast/transform_ops.rs `apply_transforms`). Threshold order is CLI, then header `idle_time_limit`, then 2.0 s. Creates a hidden `.bak` if none exists, writes atomically, works on `.cast` and `.cast.gz` and keeps the format.
- Recording flow end (src/recording.rs `record` / `finish`): lock removed, path recovery, `finalize_recording` (gzip when `[storage] compression = "gzip"`, src/recording/finalize.rs), stop banner, `maybe_auto_analyze` (`[recording] auto_analyze`, default false), storage warning.
- Verified on main 2026-11-06: compression converts an existing `.x.cast.bak` to `.x.cast.gz.bak` (`src/storage/compress/file.rs` `compress_backup`, reached via `finalize_recording`). The criteria above still make the Implementer prove it for the optimize backup with a test.
- Sync's later auto push hooks in after compress, which is why this has to land first.

## Resolved Open Questions

All five questions were answered by the user on 2026-11-06, see Fixed Decisions.

---
**Sign-off:** Approved by user (2026-11-06)
