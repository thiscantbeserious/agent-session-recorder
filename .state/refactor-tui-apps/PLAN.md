# Implementation Plan: Modularize TUI Apps into Shared Framework

Based on ADR at `.state/refactor-tui-apps/ADR.md` (Revision 7, Option C).

**Verification command (must pass after every stage):**
```
cargo test && cargo clippy -- -D warnings && cargo insta test --check
```

---

## Stage 1: Rename `event.rs` to `event_bus.rs`

Infrastructure rename. No logic changes. Updates all import paths directly (no shim).

- [x] Rename `src/tui/event.rs` to `src/tui/event_bus.rs`
- [x] Update `src/tui/mod.rs`: change `pub mod event;` to `pub mod event_bus;`
- [x] Update `src/tui/app.rs`: change `use super::event::{Event, EventHandler};` to `use super::event_bus::{Event, EventHandler};`
- [x] Update `src/tui/list_app.rs`: change `use super::event::Event;` to `use super::event_bus::Event;`
- [x] Update `src/tui/cleanup_app.rs`: change `use super::event::Event;` to `use super::event_bus::Event;`
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

**Files created:** none
**Files modified:** `src/tui/event_bus.rs` (renamed from `event.rs`), `src/tui/mod.rs`, `src/tui/app.rs`, `src/tui/list_app.rs`, `src/tui/cleanup_app.rs`
**What to verify:** All imports resolve. All tests pass. No dead code warnings from old `event` module.

---

## Stage 2: Extract generic `lru_cache/` module from `preview_cache.rs`

Replace the preview-specific `PreviewCache` with a generic `AsyncLruCache<K, V>`. Keep a `PreviewCache` type alias in the new module so all existing call sites compile unchanged.

- [x] Create `src/tui/lru_cache/` directory
- [x] Create `src/tui/lru_cache/mod.rs` (~30 lines): `pub mod cache; pub mod worker;` plus re-exports of `AsyncLruCache` and `PreviewCache` type alias
- [x] Create `src/tui/lru_cache/worker.rs` (~60 lines): `LoadResult<K, V>` struct, `worker_loop` function taking `Receiver<K>`, `Sender<LoadResult<K,V>>`, and a loader closure
- [x] Create `src/tui/lru_cache/cache.rs` (~100 lines): `AsyncLruCache<K, V>` struct with `new(max_size, loader)`, `poll()`, `get()`, `request()`, `prefetch()`, `invalidate()`, `is_pending()`; internal `insert()` and `touch()` methods
- [x] Add `PreviewCache` type alias in `src/tui/lru_cache/mod.rs`: `pub type PreviewCache = AsyncLruCache<String, SessionPreview>;` with a constructor helper or `Default` impl
- [x] Update `src/tui/mod.rs`: replace `pub mod preview_cache;` with `pub mod lru_cache;`
- [x] Update `src/tui/list_app.rs`: change `use super::preview_cache::PreviewCache;` to `use super::lru_cache::PreviewCache;`
- [x] Update `src/tui/cleanup_app.rs`: change `use super::preview_cache::PreviewCache;` to `use super::lru_cache::PreviewCache;`
- [x] Migrate tests from old `preview_cache.rs` into `src/tui/lru_cache/cache.rs` (adapted for generic API)
- [x] Delete `src/tui/preview_cache.rs`
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

**Files created:** `src/tui/lru_cache/mod.rs`, `src/tui/lru_cache/cache.rs`, `src/tui/lru_cache/worker.rs`
**Files deleted:** `src/tui/preview_cache.rs`
**Files modified:** `src/tui/mod.rs`, `src/tui/list_app.rs`, `src/tui/cleanup_app.rs`
**What to verify:** `PreviewCache` type alias works as drop-in replacement. All preview loading still works. Generic `AsyncLruCache` has unit tests. No changes to behavior.

---

## Stage 3: Split `widgets/file_explorer.rs` into `file_item.rs` and `preview.rs`

Extract `FileItem` and `SessionPreview` into their own files under `widgets/`. The remaining `file_explorer.rs` keeps the state machine, renderer, sort enums, and inline tests.

### Stage 3a: Extract `widgets/file_item.rs`

- [x] Create `src/tui/widgets/file_item.rs` (~90 lines): move `FileItem` struct, `FileItem::new()`, `impl From<SessionInfo>`, and `format_size()` (the one from `file_explorer.rs`, lines 1038-1052)
- [x] Update `src/tui/widgets/file_explorer.rs`: remove `FileItem`, `From<SessionInfo>`, `format_size()`; add `use super::file_item::{FileItem, format_size};`
- [x] Update `src/tui/widgets/mod.rs`: add `pub mod file_item;` and `pub use file_item::{FileItem, format_size};`
- [x] Ensure `format_size` tests from `file_explorer.rs` move to `file_item.rs` inline tests
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

**Files created:** `src/tui/widgets/file_item.rs`
**Files modified:** `src/tui/widgets/file_explorer.rs`, `src/tui/widgets/mod.rs`
**What to verify:** All imports of `FileItem` via `agr::tui::widgets::FileItem` still resolve. `format_size` tests pass. Snapshot tests unchanged.

### Stage 3b: Extract `widgets/preview.rs`

- [x] Create `src/tui/widgets/preview.rs` (~280 lines): move `SessionPreview` struct and full impl (`load`, `load_streaming`, `parse_event_minimal`, `styled_line_to_ratatui`, `to_ratatui_color`, `format_duration`) from `file_explorer.rs`
- [x] Move the `use crate::terminal::{Color, StyledLine};` and `use crate::asciicast::EventType;` imports to `preview.rs` (only if no longer needed in `file_explorer.rs`)
- [x] Update `src/tui/widgets/file_explorer.rs`: remove `SessionPreview` and all its impl methods; add `use super::preview::SessionPreview;` if needed internally
- [x] Update `src/tui/widgets/mod.rs`: add `pub mod preview;` and `pub use preview::SessionPreview;`
- [x] Do NOT add `prefetch_adjacent_previews()` or `extract_preview()` stubs -- those come in Stage 5 when extracting from `list_app.rs`
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

**Files created:** `src/tui/widgets/preview.rs`
**Files modified:** `src/tui/widgets/file_explorer.rs`, `src/tui/widgets/mod.rs`
**What to verify:** All imports of `SessionPreview` via `agr::tui::widgets::SessionPreview` still resolve. Preview loading in integration tests still passes. `file_explorer.rs` is now ~650 lines production code (plus ~400 lines inline tests).

---

## Stage 4: Convert `app.rs` to `app/` directory module with `TuiApp` trait

Move `app.rs` into `app/mod.rs`. Define the `TuiApp` trait. No consumers yet -- both apps still use their own `run()`. This stage only creates the framework skeleton.

- [ ] Create `src/tui/app/` directory
- [ ] Move `src/tui/app.rs` content into `src/tui/app/mod.rs`
- [ ] Delete `src/tui/app.rs`
- [ ] Add `TuiApp` trait definition in `app/mod.rs` with required methods: `app()`, `shared_state()`, `is_normal_mode()`, `set_normal_mode()`, `handle_key()`, `draw()`; and default `run()` method
- [ ] Create `src/tui/app/shared_state.rs` (~60 lines): `SharedState` struct with fields `explorer`, `search_input`, `agent_filter_idx`, `available_agents`, `status_message`, `preview_cache`; `SharedState::new(items)` constructor with agent collection logic; `apply_agent_filter()` method
- [ ] Create `src/tui/app/keybindings.rs` (~120 lines): `SharedMode` enum (Normal, Search, AgentFilter, Help, ConfirmDelete); `KeyResult` enum (Consumed, NotConsumed); `handle_shared_key()` function stub (will be populated in Stage 5)
- [ ] Create `src/tui/app/layout.rs` (~50 lines): `build_explorer_layout()` function returning 3-chunk vertical layout (Min(1) / Length(1) / Length(1))
- [ ] Create `src/tui/app/list_view.rs` (~60 lines): `render_explorer_list()` function that configures and renders `FileExplorerWidget` into the explorer chunk
- [ ] Create `src/tui/app/modals.rs` (~70 lines): `center_modal()` utility, `render_confirm_delete_modal()` shared between both apps
- [ ] Create `src/tui/app/status_footer.rs` (~80 lines): `render_status_line()` and `render_footer()` functions (initially stubs or extracted from common patterns)
- [ ] Add sub-module declarations and re-exports in `app/mod.rs`
- [ ] Ensure `src/tui/mod.rs` still has `pub mod app;` (Rust resolves this to `app/mod.rs` automatically)
- [ ] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

**Files created:** `src/tui/app/mod.rs`, `src/tui/app/shared_state.rs`, `src/tui/app/keybindings.rs`, `src/tui/app/layout.rs`, `src/tui/app/list_view.rs`, `src/tui/app/modals.rs`, `src/tui/app/status_footer.rs`
**Files deleted:** `src/tui/app.rs`
**Files modified:** `src/tui/mod.rs` (if needed for re-exports)
**What to verify:** `App` struct still works exactly as before. New trait and modules compile but are not yet used by `list_app` or `cleanup_app`. All existing tests pass unchanged.

---

## Stage 5: Extract shared logic from `list_app.rs` into `app/` framework

This is the primary extraction stage. Pull duplicated handler functions from `list_app.rs` into the `app/` framework files. `list_app.rs` implements `TuiApp` trait.

### Stage 5a: Populate `keybindings.rs` with shared key handlers

- [x] Extract `handle_search_key()` logic from `list_app.rs` into `app/keybindings.rs` as part of `handle_shared_key()` dispatch
- [x] Extract `handle_agent_filter_key()` logic from `list_app.rs` into `app/keybindings.rs`
- [x] Extract `handle_help_key()` logic from `list_app.rs` into `app/keybindings.rs`
- [x] Extract navigation key handling (up/down/pgup/pgdn/home/end) from `list_app.rs` into `app/keybindings.rs`
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

### Stage 5b: Populate view helpers and move `prefetch`/`extract` to `widgets/preview.rs`

- [x] Populate `app/layout.rs`: extract the 3-chunk vertical layout from `list_app.rs` `draw()` into `build_explorer_layout()`
- [x] Populate `app/list_view.rs`: extract `FileExplorerWidget` configuration + rendering from `list_app.rs` `draw()` into `render_explorer_list()`
- [x] Populate `app/status_footer.rs`: extract status line and footer rendering from `list_app.rs` `draw()`
- [x] Populate `app/modals.rs`: extract `center_modal()` pattern and shared confirm-delete modal rendering from `list_app.rs`
- [x] Move `prefetch_adjacent_previews()` from `list_app.rs` to `src/tui/widgets/preview.rs` (free function taking `&FileExplorer` and `&mut PreviewCache`)
- [x] Move preview extraction pattern from `list_app.rs` `draw()` into `extract_preview()` in `src/tui/widgets/preview.rs`
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

### Stage 5c: Make `ListApp` implement `TuiApp`

- [x] Add `shared_state: SharedState` field to `ListApp`, replacing individual `explorer`, `search_input`, `agent_filter_idx`, `available_agents`, `status_message`, `preview_cache` fields
- [x] Implement `TuiApp` trait for `ListApp`: `app()`, `shared_state()`, `is_normal_mode()`, `set_normal_mode()`, `handle_key()`, `draw()`
- [x] Replace `ListApp::run()` with the trait default `run()` (remove the custom `run` method; if the trait default is not sufficient, keep a thin override that calls the default)
- [x] Update `ListApp::handle_key()` to call `handle_shared_key()` first, then handle app-specific modes (ContextMenu, OptimizeResult, ConfirmDelete with app-specific delete logic)
- [x] Update `ListApp::draw()` to use `build_explorer_layout()`, `render_explorer_list()`, `render_status_line()`, `render_footer()` from framework
- [x] Keep `ListApp::render_help_modal()`, `render_context_menu_modal()`, `render_optimize_result_modal()` as public static methods on `ListApp` (snapshot test compatibility)
- [x] Keep `ContextMenuItem`, `ContextMenuitem::ALL`, `OptimizeResultState` in `list_app.rs`
- [x] Update `ListApp::new()` to construct `SharedState` internally
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`
- [x] Verify line count: `list_app.rs` is ~1080 lines production (~1180 with tests) -- higher than the ~400 estimate because the 3 public static modal methods + their content builders account for ~325 lines, and the 8 session actions (play/copy/delete/restore/optimize/analyze/add_marker + analyze result handler) account for ~200 lines, all app-specific code that cannot be shared

**Files modified:** `src/tui/list_app.rs`, `src/tui/mod.rs`, `src/commands/list.rs`
**What to verify:** All snapshot tests pass byte-for-byte. `ListApp` public API unchanged (`new()`, `run()`, `set_agent_filter()`, static modal methods). `TuiApp` re-exported from `tui/mod.rs` and imported in `commands/list.rs` for trait method access.

---

## Stage 6: Migrate `cleanup_app.rs` to `TuiApp` trait

Same pattern as Stage 5c but for `CleanupApp`. The shared handlers are already extracted, so this stage is primarily about wiring.

- [x] Add `shared_state: SharedState` field to `CleanupApp`, replacing individual `explorer`, `search_input`, `agent_filter_idx`, `available_agents`, `status_message`, `preview_cache` fields
- [x] Implement `TuiApp` trait for `CleanupApp`: `app()`, `shared_state()`, `is_normal_mode()`, `set_normal_mode()`, `handle_key()`, `draw()`
- [x] Replace `CleanupApp::run()` with the trait default `run()`
- [x] Update `CleanupApp::handle_key()` to call `handle_shared_key()` first, then handle app-specific modes (GlobSelect, ConfirmDelete with bulk-delete logic)
- [x] Update `CleanupApp::draw()` to use `build_explorer_layout()`, `render_explorer_list()`, `render_status_line()`, `render_footer()` from framework
- [x] Keep `CleanupApp`-specific help modal, glob-select modal, and bulk-delete confirm modal in `cleanup_app.rs`
- [x] Keep `cleanup_app.rs`'s own `format_size()` (uses `humansize` crate, different from `widgets/file_item.rs` version)
- [x] Update `CleanupApp::new()` to construct `SharedState` internally
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`
- [x] Verify line count: `cleanup_app.rs` is ~768 lines (688 production + 80 tests) -- higher than the ~350 estimate because the help modal content builder accounts for ~75 lines, glob matching functions account for ~45 lines, select_by_glob accounts for ~50 lines, and the bulk-delete confirm modal + status/footer helpers account for ~110 lines, all app-specific code that cannot be shared

**Files modified:** `src/tui/cleanup_app.rs`, `src/commands/cleanup.rs`
**What to verify:** `CleanupApp` public API unchanged (`new()`, `run()`, `files_were_deleted()`). `TuiApp` re-exported from `tui/mod.rs` and imported in `commands/cleanup.rs` for trait method access. Both apps behave identically to before.

---

## Stage 7: Update `mod.rs` re-exports and verify all external imports

Ensure all public re-exports are correct and all external import paths work.

- [x] Update `src/tui/mod.rs`: add re-exports for new modules (`app::TuiApp`, `app::SharedState`, `lru_cache::PreviewCache`, `lru_cache::AsyncLruCache` if needed)
- [x] Update `src/tui/widgets/mod.rs`: verify re-exports include `file_item::FileItem`, `file_item::format_size`, `preview::SessionPreview`, `file_explorer::{FileExplorer, FileExplorerWidget, SortDirection, SortField}`, `logo::Logo`
- [x] Verify all external import paths work:
  - `agr::tui::ListApp`
  - `agr::tui::CleanupApp`
  - `agr::tui::list_app::ListApp`
  - `agr::tui::list_app::OptimizeResultState`
  - `agr::tui::widgets::FileItem`
  - `agr::tui::widgets::FileExplorer`
  - `agr::tui::widgets::FileExplorerWidget`
  - `agr::tui::widgets::SessionPreview`
  - `agr::tui::widgets::SortField`
  - `agr::tui::widgets::SortDirection`
  - `agr::tui::widgets::Logo`
- [x] Verify: `cargo test && cargo clippy -- -D warnings && cargo insta test --check`

**Files modified:** `src/tui/mod.rs`, `src/tui/widgets/mod.rs`
**What to verify:** All test files compile. All command files compile. No unused import warnings. No broken re-export paths.

---

## Stage 8: Final cleanup and verification

Remove any temporary scaffolding. Verify all acceptance criteria.

- [x] Remove any temporary type aliases that were used as intermediate compilation aids
- [x] Remove any `#[allow(dead_code)]` annotations added during refactoring (keep the existing one in `mod.rs` only if still needed)
- [x] Remove any TODO comments left during staged implementation
- [x] Run `cargo clippy -- -D warnings` and fix any warnings
- [x] Run `cargo test` -- all unit tests pass
- [x] Run `cargo insta test --check` -- all snapshot tests pass byte-for-byte
- [x] Verify line counts:
  - `src/tui/list_app.rs` 1155 lines (production + ~100 lines tests) -- higher than ~400 estimate because 3 public static modal methods + content builders ~325 lines, and 8 session actions ~200 lines, all app-specific
  - `src/tui/cleanup_app.rs` 765 lines (production + ~80 lines tests) -- higher than ~350 estimate because help modal ~75 lines, glob matching ~45 lines, select_by_glob ~50 lines, bulk-delete modal + status/footer ~110 lines, all app-specific
  - `src/tui/app/mod.rs` 271 lines
  - `src/tui/app/keybindings.rs` 565 lines (includes ~360 lines of unit tests)
  - `src/tui/app/shared_state.rs` 62 lines
  - `src/tui/app/layout.rs` 43 lines
  - `src/tui/app/list_view.rs` 29 lines
  - `src/tui/app/modals.rs` 110 lines
  - `src/tui/app/status_footer.rs` 77 lines
  - `src/tui/lru_cache/mod.rs` 23 lines
  - `src/tui/lru_cache/cache.rs` 250 lines (includes ~140 lines of unit tests)
  - `src/tui/lru_cache/worker.rs` 31 lines
  - `src/tui/event_bus.rs` 146 lines
  - `src/tui/widgets/file_item.rs` 116 lines
  - `src/tui/widgets/preview.rs` 351 lines
  - `src/tui/widgets/file_explorer.rs` 1069 lines (with inline tests, accepted exception)
  - All other files under 400 lines
- [x] Verify zero code duplication between `list_app.rs` and `cleanup_app.rs` for shared patterns (search, agent filter, help, navigation, modal centering, preview prefetch) -- only structural similarities remain (Mode enum conversion methods, draw preamble), which differ in app-specific variants and cannot be further shared
- [x] Verify no shim files exist
- [x] Verify no backward-compatibility type aliases remain in final state
- [x] Verify no dead code warnings -- remaining `#[allow(dead_code)]` on 4 genuinely unused framework items: `extract_preview`, `clear_area`, `render_footer`, and `CleanupApp.storage` field

**Files modified:** potentially any file with temporary scaffolding
**What to verify:** Full acceptance criteria from REQUIREMENTS.md. Clean `cargo clippy`. Clean `cargo test`. Clean `cargo insta test --check`. All files within size limits.

---

## Stage Dependency Graph

```
Stage 1 (event_bus rename)
    |
Stage 2 (lru_cache extraction)
    |
Stage 3a (file_item.rs) --> Stage 3b (preview.rs)
    |
Stage 4 (app/ directory + TuiApp trait skeleton)
    |
Stage 5a (keybindings) --> Stage 5b (view helpers + preview functions) --> Stage 5c (ListApp implements TuiApp)
    |
Stage 6 (CleanupApp implements TuiApp)
    |
Stage 7 (re-exports verification)
    |
Stage 8 (final cleanup)
```

Stages 1, 2, and 3 are independent of each other and could theoretically be done in parallel, but sequential execution is recommended to keep diffs reviewable. Stage 4 depends on Stages 1-3 being complete (the `app/mod.rs` needs to import from `event_bus` and `lru_cache`). Stages 5 and 6 are strictly sequential. Stages 7 and 8 are final verification.
