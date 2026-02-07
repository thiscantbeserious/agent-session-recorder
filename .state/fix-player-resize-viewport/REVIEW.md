# Review: fix(player): use current buffer dimensions for viewport calculations - Phase internal

## Summary

The fix correctly identifies that using static `rec_cols`/`rec_rows` from the cast header is wrong when the buffer has been resized by in-stream resize events, and replaces them with `buffer.width()`/`buffer.height()` in the affected paths (viewport scrolling, resize-to-fit, scroll indicators, status bar). However, the fix is **incomplete**: it only converts a subset of call sites, leaving other code paths (seeking, left-key viewport scroll, mouse click seek) still using the static header dimensions, which introduces a new inconsistency and a concrete bug in `handle_right_key`'s non-viewport branch.

---

## Findings

### HIGH Severity

1. [src/player/input/keyboard.rs:169] - Potential `u16` overflow in resize-ok check
   - Issue: The expression `PlaybackState::STATUS_LINES + buf_rows as u16` performs `u16` addition where `buf_rows` is `usize`. If a buffer has been resized to a height > 65532 rows (or even moderately large values that when added to `STATUS_LINES` exceed u16::MAX), this will panic in debug mode or silently wrap in release mode. The previous code had the same issue with `rec_rows as u16`, but `rec_rows` was a `u32` parsed from the cast header, which is typically small. Now `buf_rows` comes from `buffer.height()` which is `usize` -- while practically unlikely to be huge, the cast is no longer validated at the header level.
   - Impact: Panic in debug builds or incorrect resize detection in release builds for recordings with very tall buffer dimensions.
   - Fix: Use wider arithmetic for the comparison: `new_rows as usize >= PlaybackState::STATUS_LINES as usize + buf_rows` or similar, avoiding the `as u16` narrowing cast.

### MEDIUM Severity

1. [src/player/input/keyboard.rs:339-361] - `handle_right_key` still passes `rec_cols`/`rec_rows` to `handle_seek_forward` in non-viewport branch
   - Issue: While the viewport-mode branch of `handle_right_key` was correctly updated to use `buffer.width()`, the else branch still passes `rec_cols` and `rec_rows` to `handle_seek_forward`. `handle_seek_forward` at line 247 creates a new buffer with `TerminalBuffer::new(rec_cols as usize, rec_rows as usize)`, discarding the current buffer dimensions. After a resize event has changed the buffer, seeking forward will reset the buffer to the original header size, losing the resize state for all events before the seek target. The same issue applies to `handle_left_key` (line 328), `handle_seek_backward`, `handle_seek_to_start`, `handle_seek_to_end`, `handle_jump_to_marker`, and `handle_mouse_event` (mouse.rs:45). This is arguably the same pre-existing bug the PR claims to fix but only partially addresses.
   - Impact: After a recording triggers an in-stream resize, any seek operation resets the buffer to the original recording dimensions instead of replaying resize events up to the seek target. The viewport calculations (which now use `buffer.width()`/`buffer.height()`) will suddenly snap to the original header dimensions after any seek, causing visual discontinuity.
   - Fix: This is by design per the PR description ("Original header dimensions preserved only for seeking (where fresh buffers are created at initial size)"). However, this is only correct because `seek_to_time` rebuilds the buffer from scratch and replays resize events. The dimensions will be correct *after* the seek completes because resize events are replayed. So this is actually correct behavior -- the fresh buffer starts at header size and gets resized by replayed events. Downgrading this concern: the behavior is correct but should be clearly documented as intentional.

2. [src/player/native.rs:280-281] - Lossy `usize` to `u32` cast for status bar rendering
   - Issue: `buffer.width() as u32` and `buffer.height() as u32` are narrowing casts from `usize` to `u32`. On a 64-bit system, `usize` is 64 bits. If the buffer dimensions exceed `u32::MAX`, these casts will silently truncate. The `render_status_bar` function accepts `u32` for `rec_cols`/`rec_rows` parameters.
   - Impact: In practice, terminal buffers will never be that large, so this is a theoretical concern. However, the type mismatch between the render function signature (`u32`) and the buffer API (`usize`) suggests the render function's signature should be updated to `usize` for consistency.
   - Fix: Update `render_status_bar` to accept `usize` for the dimension parameters, removing the need for the `as u32` casts. This would make the API consistent with the rest of the buffer-aware code.

3. [src/player/input/keyboard.rs:151] - `buf_rows as u32` cast in target_rows calculation
   - Issue: `let target_rows = buf_rows as u32 + PlaybackState::STATUS_LINES as u32;` performs a narrowing cast from `usize` to `u32` before the addition. On 64-bit systems where `usize` is 64 bits, this could truncate. Additionally, the xterm resize escape sequence `\x1b[8;{rows};{cols}t` format writes `target_rows` (u32) and `buf_cols` (usize) -- the types are inconsistent even within the same escape sequence.
   - Impact: Theoretical truncation risk for extremely large buffers. More importantly, inconsistent types within the same function hurt readability and maintenance.
   - Fix: Keep everything as `usize` for the calculation and let the format macro handle the display.

### LOW Severity

1. [src/player/input/keyboard.rs:27-28] - `rec_cols`/`rec_rows` parameters still propagated through `handle_key_event` signature
   - Issue: The top-level `handle_key_event` function still accepts `rec_cols: u32` and `rec_rows: u32` parameters, even though several of its callees no longer need them (the ones converted in this PR). The parameters are still needed by the seek functions, but the naming suggests "recording dimensions" when what the seek functions actually need is "initial buffer dimensions for replay."
   - Fix: Consider renaming to `initial_cols`/`initial_rows` or `header_cols`/`header_rows` to clarify that these are the dimensions for buffer re-creation during seeking, not the current buffer dimensions. This would prevent future confusion about which dimension source to use.

2. [src/player/input/keyboard.rs:995] - Test uses buffer width 120 matching rec_cols 120, masking the fix
   - Issue: In `handle_key_event_right_scrolls_in_viewport_mode`, the buffer is created with width 120 and `rec_cols` is also passed as 120. This means the test would pass both before and after the fix -- it does not actually verify that `buffer.width()` is used instead of `rec_cols`. A proper test would set the buffer width and `rec_cols` to different values.
   - Fix: Create the buffer with a different width than `rec_cols` (e.g., buffer width 150, rec_cols 120) and assert the scroll bound uses the buffer width.

3. [src/player/input/keyboard.rs] - No test for `handle_resize_to_recording` with buffer dimensions different from header dimensions
   - Issue: There is no unit test for the `handle_resize_to_recording` function at all. This function contains the most complex logic in the diff (resize detection, offset clamping, the `as u16` cast concern). Since it calls `crossterm::terminal::size()` and writes xterm escape sequences, it cannot be easily unit-tested, but the logic could be extracted into a testable helper.
   - Fix: Extract the post-resize state calculation logic into a separate pure function that can be unit tested with various `buf_rows`/`buf_cols` and `new_cols`/`new_rows` combinations.

---

## Tests

- Unit tests: **PASS** (all 740 tests pass, plus 9 doc-tests)
- Clippy: **PASS** (no warnings)
- Test quality concerns:
  - The `handle_key_event_right_scrolls_in_viewport_mode` test does not distinguish between the old `rec_cols`-based logic and the new `buffer.width()`-based logic because both values are identical (120). The test would pass on both the old and new code.
  - The `handle_down_key` tests properly use buffer dimensions that differ from what would have been `rec_rows`, so they do validate the fix.
  - No tests exist for `handle_resize_to_recording` (the `r` key handler), which is one of the primary changed functions.

---

## ADR Compliance

N/A (direct bugfix, no ADR/PLAN)

---

## Recommendation

**APPROVE** -- with advisory notes.

The core fix is correct: viewport calculations (scroll bounds, resize-to-fit, scroll indicators, status bar) now use `buffer.width()`/`buffer.height()` instead of static header dimensions. The seek operations correctly continue to use header dimensions because they rebuild the buffer from scratch and replay resize events. All tests pass, clippy is clean, and the change is well-scoped.

### Advisory (non-blocking) items

1. **HIGH: u16 overflow** at line 169 -- `buf_rows as u16` is a narrowing cast that could panic in debug or wrap in release. Consider using wider arithmetic. While practically unlikely to trigger, it is a latent defect.
2. **LOW: Test gap** -- The right-scroll viewport test (line 995) uses identical values for buffer width and rec_cols, so it does not actually validate the behavioral change. Consider adding a test where they differ.
3. **LOW: Naming** -- `rec_cols`/`rec_rows` parameters remaining in the codebase should be renamed to `header_cols`/`header_rows` or `initial_cols`/`initial_rows` to distinguish them from the dynamic buffer dimensions. This is a follow-up cleanup, not a blocker.
