# Review: PR #125 -- refactor(tui): Modularize list_app and cleanup_app into Shared TUI Components

**Reviewer:** Adversarial Internal Review (Phase 1)
**Branch:** refactor-tui-apps
**Date:** 2026-02-07
**Commits reviewed:** 13 (5b338ce..a43d39c)

---

## Summary

PR #125 implements the ADR for modularizing `list_app.rs` (1352 lines) and `cleanup_app.rs` (879 lines) into a shared TUI framework via a `TuiApp` trait. The changes span 28 files across 8 stages:

- **Stage 1:** Renamed `event.rs` to `event_bus.rs` (pure rename, 100% similarity)
- **Stage 2:** Extracted generic `AsyncLruCache<K, V>` from `preview_cache.rs` into `lru_cache/` module
- **Stage 3:** Split `widgets/file_explorer.rs` (1456 lines) into `file_item.rs`, `preview.rs`, and a slimmer `file_explorer.rs`
- **Stage 4:** Created `app/` directory module with `TuiApp` trait, `SharedState`, `keybindings`, `layout`, `list_view`, `modals`, `status_footer`
- **Stages 5-6:** Made `ListApp` and `CleanupApp` implement `TuiApp` trait
- **Stages 7-8:** Updated re-exports, final cleanup

**Test Results:**
- `cargo test`: 785 unit tests + 13 integration tests + 10 doc-tests -- all pass
- `cargo clippy -- -D warnings`: clean, zero warnings
- `cargo insta test --check`: clean, no snapshot diffs

---

## Findings

### MEDIUM-1: Identical functions `apply_search_filter` and `apply_live_search` in keybindings.rs

**Severity:** MEDIUM
**File:** `src/tui/app/keybindings.rs`, lines 90-109
**Description:** Two functions with different names contain byte-for-byte identical bodies:

```rust
fn apply_search_filter(state: &mut SharedState) {
    if state.search_input.is_empty() {
        state.explorer.set_search_filter(None);
    } else {
        state.explorer.set_search_filter(Some(state.search_input.clone()));
    }
}

fn apply_live_search(state: &mut SharedState) {
    if state.search_input.is_empty() {
        state.explorer.set_search_filter(None);
    } else {
        state.explorer.set_search_filter(Some(state.search_input.clone()));
    }
}
```

This is a verbatim duplication within the same file. One of these should be removed and the other reused. The semantic distinction (commit-on-enter vs live-as-you-type) does not manifest in any code difference.

**Suggested fix:** Delete `apply_live_search()` and replace its two call sites (Backspace handler, Char handler) with `apply_search_filter()`. Or rename the surviving function to something neutral like `sync_search_filter()`.

---

### MEDIUM-2: `center_modal()` extracted but not used by the modals that need it most

**Severity:** MEDIUM
**File:** `src/tui/app/modals.rs` (defines `center_modal`), `src/tui/list_app.rs` (lines 180-184, 214-219, 249-256), `src/tui/cleanup_app.rs` (lines 491-495, 593-597)
**Description:** The ADR mandated extracting `center_modal()` into `app/modals.rs` to eliminate the duplicated modal centering pattern. The function was extracted and is used by `render_confirm_delete_modal` in `modals.rs` itself, but all five other modal rendering functions still perform manual modal centering with inline arithmetic:

```rust
// list_app.rs render_help_modal (line 180-184)
let modal_width = 60.min(area.width.saturating_sub(4));
let modal_height = 28.min(area.height.saturating_sub(4));
let x = (area.width - modal_width) / 2;
let y = (area.height - modal_height) / 2;
let modal_area = Rect::new(x, y, modal_width, modal_height);
```

This is the exact pattern `center_modal()` was designed to replace. There are 5 copies of this pattern across list_app.rs (3 modals) and cleanup_app.rs (2 modals), none of which use the extracted utility.

**Suggested fix:** Replace the inline centering arithmetic in all 5 modal functions with `center_modal(area, width, height)`. Note: the manual versions do NOT add `area.x`/`area.y` offsets, while `center_modal()` does. This means the manual versions have a latent bug when rendered with a non-zero-origin area (unlikely in practice since `frame.area()` is always at origin, but the extracted function is more correct).

---

### MEDIUM-3: Dead trait methods `is_normal_mode()` and `set_normal_mode()` on TuiApp

**Severity:** MEDIUM
**File:** `src/tui/app/mod.rs`, lines 225-228
**Description:** The `TuiApp` trait requires implementations for `is_normal_mode()` and `set_normal_mode()`:

```rust
pub trait TuiApp {
    fn is_normal_mode(&self) -> bool;
    fn set_normal_mode(&mut self);
    // ...
}
```

Both methods are implemented by `ListApp` and `CleanupApp`, but neither method is called anywhere in the codebase (not by the default `run()`, not by keybindings, not by any framework module). They are dead code on the trait contract, forcing implementors to write code that is never used.

**Suggested fix:** Either remove these methods from the trait (breaking change for future implementors, but currently no external consumers), or use them in the default `run()` method (e.g., reset to normal mode on quit). If they are intended for future use, add a `// TODO` comment explaining the intended purpose.

---

### MEDIUM-4: `#[allow(dead_code)]` annotations on 4 framework items mask incomplete integration

**Severity:** MEDIUM
**Files:** `src/tui/widgets/preview.rs:309`, `src/tui/app/modals.rs:33`, `src/tui/app/status_footer.rs:32`, `src/tui/cleanup_app.rs:78`
**Description:** Four items carry `#[allow(dead_code)]`:

1. `extract_preview()` in `preview.rs` -- extracted but unused (both apps inline the preview extraction instead of calling this helper)
2. `clear_area()` in `modals.rs` -- extracted but unused (modal functions use `frame.render_widget(Clear, area)` directly)
3. `render_footer()` in `status_footer.rs` -- extracted but unused (both apps use `render_footer_text()` instead)
4. `CleanupApp.storage` field -- retained for "future use" but currently dead

These are framework utilities that were extracted from the apps but never wired in. The `#[allow(dead_code)]` annotations suppress the warnings rather than completing the integration. This suggests the extraction was incomplete -- the utilities were created but the apps were not updated to use them.

**Suggested fix:** Either (a) update the apps to actually use `extract_preview()`, `clear_area()`, and `render_footer()`, removing the allow annotations, or (b) remove these functions entirely if they provide no value. For `storage` on `CleanupApp`, either use it or document the planned use with a concrete issue reference.

---

### LOW-1: File sizes significantly exceed ADR estimates

**Severity:** LOW
**Files:** `src/tui/list_app.rs` (1155 lines), `src/tui/cleanup_app.rs` (765 lines), `src/tui/app/keybindings.rs` (565 lines)
**Description:** The ADR estimated `list_app.rs` at ~400 lines and `cleanup_app.rs` at ~350 lines. Actual sizes are 2.9x and 2.2x the estimates respectively. The PLAN file acknowledges these overruns and provides explanations (modal methods ~325 lines, 8 session actions ~200 lines for list_app; help modal ~75 lines, glob matching ~95 lines for cleanup_app).

While the explanations are reasonable -- the app-specific logic genuinely cannot be shared -- the gap between estimates and actuals is notable. `list_app.rs` at 1155 lines is still close to the original 1352, meaning only ~15% code was actually removed from this file. `cleanup_app.rs` at 765 lines is ~87% of the original 879.

This is not a functional issue, but it indicates the refactoring achieved less code reduction in the primary files than the ADR projected. The shared infrastructure adds ~7 new files totaling ~750 lines. Net effect: more total code with better structure but modest deduplication.

**Suggested fix:** No code change needed. Consider updating the ADR with a "Retrospective" section documenting the actual line counts and why they diverged from estimates, so future refactoring efforts have better baselines.

---

### LOW-2: Structural duplication in Mode enum conversion methods

**Severity:** LOW
**Files:** `src/tui/list_app.rs` (lines 47-70), `src/tui/cleanup_app.rs` (lines 42-65)
**Description:** Both apps define nearly identical `to_shared_mode()` and `from_shared_mode()` methods on their Mode enums. The `from_shared_mode()` methods are byte-for-byte identical. The `to_shared_mode()` methods differ by a single line (the app-specific variant mapping to `None`).

This is a consequence of each app having its own Mode enum that wraps SharedMode plus app-specific variants. While unavoidable without macros or more complex generics, it is residual structural duplication.

**Suggested fix:** No action required for this PR. Could be addressed in a follow-up with a declarative macro if more TUI apps are added.

---

### LOW-3: Worker thread in AsyncLruCache is not joined on Drop

**Severity:** LOW
**File:** `src/tui/lru_cache/cache.rs`, lines 42-58
**Description:** `AsyncLruCache::new()` spawns a background worker thread but does not implement `Drop` to join the thread. When the cache is dropped (app exit), the worker thread is orphaned and will exit only when `request_rx.recv()` returns an error (channel closed due to sender drop). While the thread will terminate once the sender is dropped (which happens when the cache is dropped), there is a small window where the thread continues running after the cache is gone, potentially attempting to send results on a closed channel (which it correctly ignores with `let _ = result_tx.send(...)`).

The original `preview_cache.rs` had the same behavior, so this is not a regression. The impact is negligible -- the thread will exit almost immediately after the cache is dropped.

**Suggested fix:** No action needed for this PR. For completeness, consider storing the `JoinHandle` and joining in a `Drop` impl, similar to how `EventHandler` has a `stop()` method.

---

## ADR/PLAN Compliance

| Requirement | Status | Notes |
|---|---|---|
| `event.rs` renamed to `event_bus.rs` | PASS | Pure rename, 100% similarity |
| Generic `AsyncLruCache<K, V>` replaces `PreviewCache` | PASS | Clean generic extraction with type alias |
| `file_explorer.rs` split into 3 files | PASS | `file_item.rs`, `preview.rs`, `file_explorer.rs` |
| `app/` directory with TuiApp trait | PASS | 7 framework files created |
| SharedState struct with shared fields | PASS | Clean extraction |
| Unified key dispatch via `handle_shared_key()` | PASS | Single entry point, KeyResult enum |
| `ListApp` implements `TuiApp` | PASS | Trait implemented, shared run() used |
| `CleanupApp` implements `TuiApp` | PASS | Trait implemented, shared run() used |
| Snapshot tests pass byte-for-byte | PASS | All insta snapshots clean |
| No type aliases / no shims | PASS | Zero backward-compat shims |
| Original file/struct names preserved | PASS | `ListApp`, `CleanupApp`, `list_app.rs`, `cleanup_app.rs` |
| `list_app.rs` ~400 lines | FAIL | 1155 lines (2.9x estimate) |
| `cleanup_app.rs` ~350 lines | FAIL | 765 lines (2.2x estimate) |
| All files under 400 lines (except file_explorer.rs) | FAIL | `keybindings.rs` 565 lines, `list_app.rs` 1155 lines, `cleanup_app.rs` 765 lines |
| `center_modal()` extracted to `app/modals.rs` | PARTIAL | Extracted but only used by 1 of 6 modal functions |
| `prefetch_adjacent_previews()` moved to `widgets/preview.rs` | PASS | Correctly relocated |
| `extract_preview()` moved to `widgets/preview.rs` | PARTIAL | Moved but unused (dead code) |

---

## Overall Assessment

The refactoring is structurally sound. The `TuiApp` trait pattern works correctly, the shared keybinding dispatch is clean, the generic LRU cache is a genuine improvement, and the file_explorer split is well-executed. All 808 tests pass, clippy is clean, and snapshot tests are byte-for-byte identical.

The primary concerns are (a) incomplete follow-through on the extraction -- `center_modal()`, `extract_preview()`, `clear_area()`, and `render_footer()` were extracted but not wired into their intended consumers, and (b) an internal code duplication (`apply_search_filter` / `apply_live_search`) that should not have passed the implementation stage.

The line count overruns are acknowledged in the PLAN but represent a significant deviation from the ADR's targets. The refactoring achieved better modularity and eliminated the duplicated event loop, but the primary app files retained most of their bulk.

---

## Recommendation

**PASS WITH FINDINGS**

The PR is mergeable. The findings are real but not blocking:

- **MEDIUM-1** (duplicate functions): Quick fix, should be addressed before merge.
- **MEDIUM-2** (unused center_modal): Non-blocking, but worth a follow-up task.
- **MEDIUM-3** (dead trait methods): Non-blocking, cosmetic.
- **MEDIUM-4** (dead_code annotations): Non-blocking, indicates incomplete extraction.
- **LOW-1/2/3**: Informational, no action required.

Recommendation: Fix MEDIUM-1 (trivially fixable in 30 seconds) before merge. Track MEDIUM-2 and MEDIUM-4 as follow-up cleanup items.
