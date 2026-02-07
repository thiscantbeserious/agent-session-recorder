# Review: refactor(tui): wire ListApp and CleanupApp into shared framework - Phase internal

## Summary

The commit successfully wires both `ListApp` and `CleanupApp` into the shared `TuiApp` trait framework, replacing duplicated event loops, navigation, search, agent filter, and help handlers with a unified `handle_shared_key()` dispatcher. The core dispatch pattern is sound and the `ConfirmDelete` passthrough works correctly. However, the commit introduces a behavior change (the `'f'` key guard) that violates the pure-refactoring constraint, leaves a full duplicate `SessionPreview` struct as dead code in `preview.rs`, and the app files remain well above their ADR size targets.

---

## Findings

### HIGH Severity

1. [src/tui/widgets/preview.rs:21] - Duplicate `SessionPreview` definition is dead code
   - Issue: `preview.rs` defines a complete `SessionPreview` struct with all methods (load, load_streaming, parse_event_minimal, styled_line_to_ratatui, to_ratatui_color, format_duration) -- 264 lines of production code. An identical struct with identical methods still exists in `file_explorer.rs` (line 89). The `widgets/mod.rs` re-exports `SessionPreview` from `file_explorer`, NOT from `preview`. The `lru_cache` module, `list_view.rs`, and all other consumers import the `file_explorer` version. This means the entire `SessionPreview` impl in `preview.rs` is dead code that will never be executed. Only the `prefetch_adjacent_previews` free function in `preview.rs` is actually used.
   - Impact: (1) Maintenance divergence risk -- if someone fixes a bug in one `SessionPreview`, the other won't get the fix. (2) The dead code inflates the codebase by ~240 lines and will confuse future contributors who may modify the wrong copy. (3) If the `widgets/mod.rs` re-export is later changed to point at `preview.rs`, the `file_explorer.rs` version's tests would become the dead tests and vice versa. (4) If `file_explorer.rs`'s `SessionPreview` is eventually deleted (per ADR plan), code that currently works will silently switch to the `preview.rs` version with no verification that the two implementations are truly identical.
   - Fix: Either (a) complete the extraction by removing `SessionPreview` from `file_explorer.rs` and updating the `widgets/mod.rs` re-export to `pub use preview::SessionPreview;`, or (b) remove the duplicate from `preview.rs` and leave the extraction for a subsequent commit. Option (a) is preferred per the ADR plan. Either way, there must be exactly ONE definition.

### MEDIUM Severity

1. [src/tui/app/keybindings.rs:173-176] - Behavior change violates pure-refactoring constraint
   - Issue: The `'f'` key handler in `handle_normal_navigation` adds a guard that was not present in either the old `list_app.rs` or `cleanup_app.rs`: `if state.available_agents.len() <= 1 { ... return Consumed; }`. In the old code, pressing `'f'` with only one agent (i.e., `available_agents = ["All"]`) would enter `AgentFilter` mode, where the user could cycle through a single entry. The new code instead shows "No agents to filter by" and stays in `Normal` mode. The REQUIREMENTS.md states: "Pure refactoring -- zero behavior changes."
   - Impact: A user with recordings from only one agent will see different behavior before and after this commit. While the new behavior is arguably better UX, introducing it in a refactoring commit makes it invisible to reviewers looking for intentional feature changes, and it cannot be bisected back to a feature commit.
   - Fix: Remove the guard from this commit and introduce it in a separate feature commit (or a follow-up bugfix commit) with its own test and description. Alternatively, if this is intentional, document it in the commit message as a deliberate deviation from pure refactoring.

2. [src/tui/list_app.rs / cleanup_app.rs] - Files remain 2-3x above ADR target size
   - Issue: `list_app.rs` is 1156 lines (ADR target: ~400) and `cleanup_app.rs` is 731 lines (ADR target: ~350). The commit reduced them by ~14% and ~16% respectively (from 1351 and 874), but they are still far from the stated targets. The status/footer text generation and modal overlay rendering in `draw()` remain fully duplicated between both apps, which was supposed to be extracted into the shared framework.
   - Impact: The primary goal of the refactoring (eliminating duplication, reducing file sizes) is only partially achieved. The remaining duplicated code in `draw()` -- status text generation, footer text generation, and modal overlay dispatch -- represents the majority of the remaining line count. These are exactly the patterns the shared `layout.rs`, `status_footer.rs`, and `modals.rs` modules were created to absorb.
   - Fix: This is expected to be addressed in subsequent commits. Flag it here to ensure it's tracked.

3. [src/tui/cleanup_app.rs:451 / list_app.rs:900] - `to_shared()` on ConfirmDelete sends key to shared handler before app handler
   - Issue: When mode is `ConfirmDelete`, `to_shared()` returns `Some(SharedMode::ConfirmDelete)`, so the key is first sent to `handle_shared_key` which returns `NotConsumed`. The key then falls through to the app-specific `handle_confirm_delete_key`. While this currently works correctly (shared handler returns `NotConsumed` immediately for `ConfirmDelete`), it means every keystroke in `ConfirmDelete` mode pays the cost of the shared dispatch and relies on the invariant that `SharedMode::ConfirmDelete => KeyResult::NotConsumed` never changes. If someone later adds key handling for `ConfirmDelete` in the shared handler (e.g., a shared Esc-to-Normal transition), it would silently preempt the app-specific handlers without any compile-time signal.
   - Impact: Future maintenance risk. A change to the shared `ConfirmDelete` handler could break app-specific delete confirmation logic (which differs between list and cleanup) with no type-system protection.
   - Fix: Consider having `to_shared()` return `None` for `ConfirmDelete` (like it does for `GlobSelect`, `ContextMenu`, `OptimizeResult`), since each app already has its own `handle_confirm_delete_key`. This would make the app-specific handling explicit and avoid the shared dispatch entirely for this mode. If keeping the current approach, add a comment documenting the invariant.

### LOW Severity

1. [src/tui/list_app.rs:937 / cleanup_app.rs:488] - `status_message` cloned on every draw frame
   - Issue: `let status = self.shared.status_message.clone();` creates a heap allocation for the status string on every frame render (~4 times/second at 250ms tick rate). The clone exists solely to work around the borrow checker for the `self.app.draw(|frame| {...})` closure.
   - Impact: Minor unnecessary allocation in a TUI that's not performance-critical. However, this is a pattern that appears in multiple places (also `optimize_result.clone()`) and could be avoided.
   - Fix: Use `Option::as_deref()` to borrow `&str` from `Option<String>`, or restructure to take the value out with `take()` before the closure and put it back afterward. Low priority since the TUI is not a hot path.

2. [src/tui/cleanup_app.rs:294-303] - Cleanup help modal still uses raw coordinate math instead of `center_modal()`
   - Issue: `CleanupApp::render_help_modal` manually calculates modal centering with `(area.width - modal_width) / 2` and `(area.height - modal_height) / 2`. The shared `modals::center_modal()` function was extracted precisely to eliminate this pattern, but it's only used for the list app's confirm-delete modal. The cleanup app's help modal and confirm-delete modal still use raw math.
   - Impact: Code duplication. The raw centering math doesn't account for `area.x` and `area.y` offsets (it assumes the area starts at 0,0), while `center_modal()` correctly handles non-zero offsets. If the modal is rendered within a sub-area that doesn't start at (0,0), the raw version would misposition the modal.
   - Fix: Replace manual centering in both cleanup and list help modals with `modals::center_modal()`. This should be done as part of the next commit that further extracts shared draw logic.

3. [src/tui/list_app.rs:917 / cleanup_app.rs:467] - Wildcard `_ => {}` arm hides missing mode coverage
   - Issue: Both `handle_key` implementations end their app-specific match with `_ => {}`. Since all shared modes (Search, AgentFilter, Help) are handled by `handle_shared_key` before reaching this match, the wildcard is only reached if a shared mode somehow returns `NotConsumed`. This is currently impossible (all shared handlers consume all keys), so the wildcard suppresses a useful exhaustiveness check.
   - Impact: If a new shared mode is added but forgotten in the match, the compiler won't flag it. The wildcard silently swallows keys for unhandled modes.
   - Fix: Replace `_ => {}` with explicit listing of the remaining modes: `Mode::Search | Mode::AgentFilter | Mode::Help => { /* handled by shared dispatch above */ }`. This preserves exhaustiveness checking.

---

## Tests

- Unit tests: **PASS** (788 passed, 0 failed)
- Snapshot tests: **PASS** (all snapshot_tui_test cases passed unchanged)
- Clippy: **PASS** (0 warnings with `-D warnings`)
- E2E tests: NOT RUN (requires terminal; the diff does not affect e2e test infrastructure)
- Test quality concerns:
  - No tests verify that the `to_shared()` / `from_shared()` roundtrip is consistent (i.e., `from_shared(to_shared(m).unwrap()) == m` for all shared-mapped modes)
  - No test covers the new `'f'` key guard for `available_agents.len() <= 1`
  - The duplicate `SessionPreview` in `preview.rs` has its own tests (lines 305-338) that test the dead code, giving a false sense of coverage

---

## ADR Compliance

- Implementation matches Decision: **PARTIAL** -- The `TuiApp` trait, `SharedState`, `handle_shared_key()`, `KeyResult`, and `SharedMode` all match the ADR. However, the `SessionPreview` extraction is incomplete (duplicate exists) and file sizes remain 2-3x above target.
- All PLAN stages complete: **NO** -- This is one commit in an incremental process. The `draw()` deduplication, modal extraction, and `file_explorer.rs` split are pending.
- Scope maintained: **NO** -- The `'f'` key guard is new behavior not in the ADR or REQUIREMENTS.

---

## Recommendation

**REQUEST CHANGES**

### Blocking Items

1. **Remove duplicate `SessionPreview`** -- Either complete the extraction (delete from `file_explorer.rs`, update re-exports) or remove from `preview.rs`. Two identical struct definitions in the same module tree is a correctness hazard. This must be resolved before merge.

2. **Remove or separate the `'f'` key guard** -- The `available_agents.len() <= 1` guard in `keybindings.rs` is a behavior change. Either revert it from this commit and add it in a separate commit, or explicitly acknowledge the deviation in the PR description.

### Non-Blocking Items (should be addressed in follow-up commits)

3. Replace `_ => {}` wildcard in `handle_key` with explicit mode listing
4. Consider mapping `ConfirmDelete` to `None` in `to_shared()` to make app-specific handling explicit
5. Replace raw centering math with `center_modal()` in remaining modals
