# Requirements: Gzip-Compressed Session Storage

GitHub issue: #163. Follow-up that builds on this: #164 (sync, GitHub Gists first).

## Problem Statement

Recordings are stored as plain asciicast text. Terminal output compresses very well, but
we store it uncompressed. On the maintainer's real data this is 6.8G for 721 sessions, 12
sessions over 100MB and the largest at 490MB (52MB with gzip -6, roughly 9.5x). Storage
fills up for no good reason and the planned sync feature (#164) would upload and download
far more data than necessary.

## Desired Outcome

After implementation:

1. New recordings end up on disk gzip-compressed as `<name>.cast.gz`, by default.
2. Compression is configurable (`[storage] compression = "gzip" | "none"`, default `"gzip"`).
3. Existing plain `.cast` sessions can be compressed through an explicit command and through
   an offer after upgrading. Nothing is rewritten without confirmation.
4. Every command behaves the same on plain `.cast` and compressed `.cast.gz` sessions.
5. Storage usage is tracked properly: on-disk size is the primary number, uncompressed size
   is shown where useful.
6. The compressed format and file naming are stable and sync-friendly, so #164 can build on
   them without changing either.

## Fixed Decisions (already made by the user, not up for discussion)

1. New recordings are stored gzip-compressed by default, configurable, default gzip.
2. Existing plain sessions: BOTH an explicit command (`agr compress`, with a dry-run preview
   of savings and a confirm prompt or `--yes`) AND an auto-migrate offer after upgrading.
   Nothing is rewritten silently. This must be implemented very cleanly, like the rest of
   the project.
3. Storage usage tracking is correct after compression (stats, status, list show on-disk
   size, uncompressed size is considered).
4. Every command works the same on plain and compressed sessions: play, analyze, list/ls
   TUI, cleanup, transform, marker, copy/clipboard, import, completions, file lookup and
   resolve.
5. Sync (#164) is out of scope here, but format and naming must be sync-friendly.
6. Clipboard always hands over plain asciicast, never gzip.
7. The auto-migrate offer is made at most once per agr version while plain sessions remain.
8. Import compresses when `compression = "gzip"`.
9. Uncompressed size is read without decompressing: exact size stored in the gzip header (FEXTRA) by agr, gzip trailer (ISIZE) as fallback for files compressed elsewhere. (Revised 2026-11-05 by user.)

## Scope

### In Scope

**Format and naming**
- Compressed sessions use the suffix `.cast.gz` (the `.cast` stays in the name, `.gz` is
  appended). Standard single-member gzip, readable by plain `gzip`/`zcat`.
- Output is deterministic: no mtime or filename in the gzip header, fixed compression level
  (6), so the same content always yields the same bytes (stable hashes for sync).
- A session is identified by its stem. `session.cast` and `session.cast.gz` are the same
  logical session, never listed twice.
- Auxiliary files follow the existing hidden convention (PR #160) and are derived from the
  full on-disk filename, e.g. `.session.cast.gz.bak`.

**Recording**
- Live recording stays plain `.cast` (asciinema writes plain asciicast, and lock, inode and
  header recovery rely on that).
- After the recording ends and the lock file is removed (src/recording.rs `Recorder::record`),
  the file is compressed if `compression = "gzip"`.
- Compression is atomic: written to a hidden temp file, verified (decompresses and matches
  the source line for line), renamed into place, and only then is the plain file removed.
  A failure at any step leaves the plain `.cast` untouched and prints a warning, the
  recording is never lost.

**Config**
- New `[storage] compression` key, values `"gzip"` and `"none"`, default `"gzip"`.
- Added through a versioned config migration (v4, after v3 from PR #162), following the
  existing migration pattern. Invalid values are rejected with a clear message.

**Existing sessions**
- `agr compress` (explicit command): dry-run preview (sessions affected, current size,
  projected size, savings), then confirm prompt, `--yes` skips the prompt. Optional agent
  filter. Idempotent, already-compressed files are skipped.
- Auto-migrate offer: on an interactive run, if plain sessions exist and the offer has not
  yet been made for the running agr version, show the same preview and ask. Accepting runs
  the same code path as `agr compress`. Declining is remembered per agr version, so the
  offer is re-made once after each new agr version while plain sessions remain, and never
  more than once per version. Non-interactive runs (no TTY) never prompt and never rewrite.
- Sessions that are currently being recorded (live lock file with a live PID) are skipped
  and reported as skipped. Stale locks are cleaned as today and the file is then eligible.
- Per-file failures do not abort the run, they are reported at the end (compressed, skipped,
  failed, bytes saved).

**Reading and writing (all commands)**
- One shared way to open a session for reading (transparent gunzip by extension) and one for
  writing (re-compress when the source was compressed), used by all commands instead of
  per-command special cases.
- Playback and analyze read the file as a stream through the decoder. The parser already
  accepts a `BufRead` (`AsciicastFile::parse_reader`), so no temp file is needed. Memory use
  must not be worse than for the plain file today.
- Commands that rewrite a session (transform, marker add/remove, analyze writing markers,
  rename) keep the file compressed: atomic temp + rename, same suffix.
- Backups (`.bak`) are byte copies of the on-disk file, so a backup of a `.cast.gz` is a
  gzip file as well. Restore puts the same bytes back, compression state is preserved.
- Rename preserves the full `.cast.gz` suffix and moves the hidden backup along.
- Import accepts `.cast` and `.cast.gz`, validates the header after decompression, and
  compresses the result when `compression = "gzip"` (a `.cast.gz` import under `"none"` is
  stored plain).
- Completions and file lookup/resolve (full path, `agent/file`, bare filename) accept names
  with or without `.gz`, and `session.cast` resolves to `session.cast.gz` if only that
  exists (and the other way round).
- Clipboard copy is always plain. For a `.cast.gz`, `agr copy` decompresses to a temporary
  plain `.cast` and copies that as the file reference. The text fallback copies the
  decompressed text, with the size limit applied to the decompressed size. Plain `.cast`
  sessions behave as today. The temporary file is removed afterwards, on success and on
  failure.

**Storage tracking**
- `agr status`, stats and the list/ls TUI show on-disk size (what the disk actually uses).
- Uncompressed size is shown additionally (status totals and list detail), read from the
  exact size agr stores in the gzip header (FEXTRA) without decompressing. Files compressed
  elsewhere fall back to the gzip trailer (ISIZE), shown as "4GiB+" when it clearly wrapped. Plain sessions use their
  file size.
- Size-based features (size warning, cleanup selection and "space freed") use on-disk size.
- `agr status` shows how many sessions are still plain and how much `agr compress` would
  save.

### Out of Scope

- Sync itself, Gists, backends (#164).
- Other codecs (zstd, xz). The config value is an enum so one can be added later, but only
  `gzip` and `none` are implemented.
- Compressing while recording, or any change to how asciinema is invoked.
- Decompressing everything back (`agr decompress`) as a command. Setting `compression =
  "none"` only affects new recordings. Can be a follow-up.
- Changes to the asciicast content or the lock/backup file contents.
- Recompression of already-compressed files at a different level.
- Windows support.

## Acceptance Criteria

**Recording and config**
- [ ] With default config, a finished recording exists as `<name>.cast.gz` and no `<name>.cast`
  remains.
- [ ] With `compression = "none"`, a finished recording stays `<name>.cast`.
- [ ] While recording, the file is plain `.cast` and the hidden lock file exists, after the
  recording ends the lock file is gone.
- [ ] If compression fails (e.g. temp file cannot be written), the plain `.cast` is intact,
  a warning is printed and the exit status of the recording is unchanged.
- [ ] A `.cast.gz` produced by agr is readable with `gzip -dc` and the result parses as valid
  asciicast.
- [ ] Compressing the same input twice yields byte-identical output.
- [ ] A config without `compression` is migrated to v4 and loads with `gzip`. Running the
  migration twice changes nothing. An invalid value gives a clear error.

**Explicit compress command**
- [ ] `agr compress --dry-run` lists session count, current size, projected size and savings,
  and changes nothing on disk.
- [ ] `agr compress` asks for confirmation, `n` changes nothing, `--yes` skips the prompt.
- [ ] After a successful run every eligible `.cast` is a `.cast.gz` with identical decoded
  content, and the originals are removed only after verification.
- [ ] Running it again compresses nothing and reports zero changes.
- [ ] A session with a live lock (running PID) is skipped and listed as skipped. A session
  with a stale lock is compressed.
- [ ] If both `x.cast` and `x.cast.gz` exist, neither is touched and a warning names both.
- [ ] A failure on one file does not stop the others and is reported in the summary.

**Auto-migrate offer**
- [ ] On an interactive run with plain sessions present and no offer yet made for the
  current agr version, the offer with preview is shown.
- [ ] Accepting performs the same result as `agr compress --yes`.
- [ ] Declining rewrites nothing and records the decline for the current agr version. The
  offer is not shown again on later runs of the same version.
- [ ] After upgrading to a new agr version, with plain sessions still present, the offer is
  shown once more, and again at most once for that version.
- [ ] With no TTY (pipe, CI, scripts) there is no prompt and no rewrite, and the
  per-version state is not consumed.
- [ ] With no plain sessions, no offer is shown.

**Commands work the same on `.cast` and `.cast.gz`** (each tested for both)
- [ ] `play` plays a `.cast.gz` with the same events and timing as the plain file.
- [ ] `analyze` runs on a `.cast.gz`, markers are written back, the file stays gzip.
- [ ] `transform` on a `.cast.gz` produces a `.cast.gz` with the same transformed content as
  the plain case, and the backup is created and restorable.
- [ ] `marker add/list/remove` work on `.cast.gz` and the file stays gzip.
- [ ] Rename of a `.cast.gz` keeps `.cast.gz` and moves its hidden backup.
- [ ] `list`/`ls` and the TUI file explorer show each session once, with preview and
  playback working for `.cast.gz`.
- [ ] `cleanup` selects, deletes and reports freed space for `.cast.gz`, including its hidden
  lock and backup files.
- [ ] `import` accepts `.cast` and `.cast.gz`, rejects a `.gz` whose content is not valid
  asciicast.
- [ ] With `compression = "gzip"`, importing a plain `.cast` stores `<name>.cast.gz`. With
  `"none"`, importing stores a plain `.cast`.
- [ ] Shell completions list `.cast.gz` sessions.
- [ ] File lookup resolves `agent/name`, `name.cast`, `name.cast.gz` and the absolute path to
  the same session regardless of which form exists on disk.
- [ ] A corrupt or truncated `.cast.gz` gives a clear error naming the file, not a panic and
  not a partial silent success.
- [ ] Playing or analyzing a large `.cast.gz` (generated test file of at least 100MB
  decompressed) does not use meaningfully more memory than the plain file.

**Clipboard**
- [ ] `agr copy` on a `.cast.gz` places a plain `.cast` file reference on the clipboard, and
  its content equals the decompressed session.
- [ ] The text fallback for a `.cast.gz` copies the decompressed text, and the size limit is
  checked against the decompressed size.
- [ ] `agr copy` on a plain `.cast` behaves as before.
- [ ] The decompressed copy lives in `<tmp>/agr-clipboard/`, the folder is cleared before each copy (at most one leftover file, so paste after agr exits still works), and it is removed on failure. (Changed 2026-11-05 by user, see ADR.)

**Storage tracking**
- [ ] `agr status` total size equals the sum of on-disk sizes and does not count the
  decompressed size as used space.
- [ ] Uncompressed size is shown next to the on-disk size in status and in list detail.
- [ ] Uncompressed size for a `.cast.gz` written by agr is exact (from the gzip header
  FEXTRA), without decompressing the file, also for sessions of 1GB or more.
- [ ] For a `.cast.gz` without that header field, the trailer value is used, "4GiB+" is shown
  only when the trailer value is smaller than the on-disk size.
- [ ] After `agr compress`, status shows the reduced on-disk total and zero (or reduced)
  plain sessions.
- [ ] The size warning (if enabled) and cleanup compare against on-disk size.
- [ ] Hidden lock, backup and temp files are never counted as sessions.

**Quality**
- [ ] `cargo test`, `cargo clippy` and `cargo fmt --check` pass.
- [ ] README and the config docs (`src/config/docs.rs`) describe the new key, the `.cast.gz`
  naming and `agr compress`.

## Constraints

- Live recordings must stay plain, compression is strictly post-recording.
- No in-place rewrite: always temp file, verify, atomic rename. Never delete the source
  before the compressed file is verified.
- A new gzip dependency is needed (none present in Cargo.toml today). The Architect picks
  the crate, preference for a pure-Rust one so cross-platform builds stay simple.
- Config changes go through the versioned migration system (v4).
- Hidden-file convention from PR #160 applies to all new auxiliary files (temp files too).
- Branch depends on PR #162 (config v3) being merged first or rebased onto.
- Where the per-version offer state is stored is an Architect decision.

## Context

- Verified in code: sessions are discovered by the `.cast` extension (src/storage/mod.rs
  `list_sessions`, `validate_cast_header`, src/files/lock.rs), analyze and transform warn
  when the extension is not `.cast`, backups and atomic writes derive `cast.tmp` from the
  extension (src/files/backup.rs, src/asciicast/writer.rs), and clipboard copy reads the file
  as text (src/clipboard/copy.rs). All of these assume plain `.cast` and are the touching
  points.
- Storage migrations in src/storage/migrate.rs run silently on every `StorageManager::new()`.
  Compression must NOT go there because it rewrites user data and requires confirmation.
  Only the offer plus `agr compress` rewrite files.
- `AsciicastFile::parse_reader` already takes a `BufRead`, so streaming decode fits the
  existing parser. The parsed file is held in memory in both cases.
- User data point: 6.8G, 721 sessions, largest 490MB to 52MB with gzip -6.
- Issue #163 mentions playing compressed files with standard tools such as zcat, which the
  standard single-member gzip requirement covers.

## Resolved Open Questions

1. **Streaming vs temp decompress**: stream through the decoder, no temp file. Same memory
   profile as today.
2. **`.bak` behavior**: byte copy of the on-disk file, stays gzip for gzip sessions, restore
   is byte-exact.
3. **Sessions being recorded during migration**: skipped (live lock), reported as skipped.
4. **Clipboard**: always plain, temp decompressed file for the file reference, cleaned up.
5. **Declined offer**: asked once per agr version, decline remembered per version.
6. **Import**: compress when `compression = "gzip"`.
7. **Uncompressed size**: exact size stored by agr in the gzip header (FEXTRA), trailer fallback for foreign files. (Revised 2026-11-05.)

---
**Sign-off:** Approved by user (2026-11-05)
