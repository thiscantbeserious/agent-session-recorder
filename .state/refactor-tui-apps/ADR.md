# ADR: Modularize list_app and cleanup_app into Shared TUI Components

**Status:** Accepted (Revised)
**Date:** 2026-02-07
**Revision:** 7 -- keep original file/struct names (list_app.rs/ListApp, cleanup_app.rs/CleanupApp)
**Decision:** Option C (Trait-Based Explorer Framework) with original naming

## Context

`src/tui/list_app.rs` (1352 lines) and `src/tui/cleanup_app.rs` (879 lines) share heavily duplicated code. Both exceed the 400-line target. Maintaining two near-identical implementations is error-prone.

`src/tui/widgets/file_explorer.rs` (1456 lines) is the central widget of the entire explorer framework. At nearly 4x the 400-line limit, it must be split as part of this refactoring. It contains six distinct concerns crammed into one file: a data model (`FileItem`), a streaming file parser (`SessionPreview`), sorting enums, explorer state management (`FileExplorer`), a rendering widget (`FileExplorerWidget`), and tests.

Both apps are fundamentally **session explorer views** -- they present a list of session recordings with navigation, filtering, search, and preview, but differ in the **actions** they expose (list: play/copy/optimize/analyze/restore/delete/add-marker; cleanup: multi-select/glob/bulk-delete).

### Duplication Inventory

| Concern | list_app lines | cleanup_app lines | Duplication type |
|---|---|---|---|
| `handle_search_key` | 38 | 38 | Identical |
| `handle_agent_filter_key` | 20 | 20 | Identical |
| `handle_help_key` | 4 | 4 | Identical |
| `handle_confirm_delete_key` | 12 | 12 | Structural (different delete fn) |
| `apply_agent_filter` | 7 | 7 | Identical |
| `prefetch_adjacent_previews` | 30 | 30 | Identical |
| Agent collection (constructor) | 5 | 5 | Identical |
| Event loop (`run`) | 20 | 20 | Identical structure |
| `draw` layout (3-chunk + status + footer) | ~70 | ~70 | Structural |
| Modal centering/clearing pattern | per modal | per modal | Structural |

**Shared state fields** in both apps: `App`, `FileExplorer`, mode enum, `search_input`, `agent_filter_idx`, `available_agents`, `status_message`, `PreviewCache`.

**App-specific logic:**
- `list_app` (`ListApp`): ContextMenu mode, OptimizeResult mode, play/copy/optimize/analyze/restore/delete/add-marker actions, context menu rendering
- `cleanup_app` (`CleanupApp`): GlobSelect mode, multi-select toggle/glob-select, bulk delete, `format_size`, `glob_match` utilities

### Existing Files Inventory

Every file below is explicitly accounted for in the refactoring plan.

| File | Lines | Disposition |
|---|---|---|
| `src/tui/mod.rs` | 19 | **Update** -- new module declarations + re-exports |
| `src/tui/app.rs` | 212 | **Move** to `app/mod.rs` (terminal lifecycle + TuiApp trait + re-exports) |
| `src/tui/event.rs` | 146 | **Rename/refactor** to `event_bus.rs` (standalone module) |
| `src/tui/preview_cache.rs` | 209 | **Replace** with generic `lru_cache/` module + type alias |
| `src/tui/ui.rs` | 125 | **Keep** -- UI utilities |
| `src/tui/list_app.rs` | 1351 | **Keep** -- struct is `ListApp`, implements `TuiApp` trait, no type alias |
| `src/tui/cleanup_app.rs` | 878 | **Keep** -- struct is `CleanupApp`, implements `TuiApp` trait, no type alias |
| `src/tui/widgets/mod.rs` | 11 | **Update** -- new sub-module declarations + re-exports |
| `src/tui/widgets/file_explorer.rs` | 1456 | **Split** into 3 files (see file_explorer split plan below) |
| `src/tui/widgets/logo.rs` | 229 | **Keep** |

## Constraint: Snapshot Tests Must Pass Unchanged

Snapshot tests call `ListApp::render_help_modal`, `ListApp::render_context_menu_modal`, and `ListApp::render_optimize_result_modal` as public static methods. After refactoring, these are called by their same names directly. The rendered content must be byte-for-byte identical.

Test import paths after refactoring (all call sites updated directly):
- `agr::tui::list_app::ListApp`
- `agr::tui::list_app::OptimizeResultState`
- `agr::tui::ListApp`
- `agr::tui::CleanupApp`

---

## Decision: Option C -- Trait-Based Explorer Framework with Domain-Oriented Naming

### Rationale

The user chose Option C and provided critical naming feedback: both apps are fundamentally **explorer/list views with different action sets**. The shared trait should reflect what these apps ARE -- session explorers.

### Naming Convention

| Concept | Name |
|---|---|
| Shared TUI app trait | `TuiApp` trait |
| Trait location | `app/mod.rs` |
| Trait method prefix | `TuiApp::draw_content`, `TuiApp::handle_key` |
| Handler pattern | free functions grouped by concern |
| List struct | `ListApp` struct |
| List file | `list_app.rs` |
| Cleanup struct | `CleanupApp` struct |
| Cleanup file | `cleanup_app.rs` |
| Terminal lifecycle | `App` struct in `app/mod.rs` |
| Shared framework dir | `app/` |

The `app/` directory is the shared TUI app framework. `App` = terminal lifecycle struct. `TuiApp` = the trait that apps implement. The original file names `list_app.rs` and `cleanup_app.rs` are retained -- they implement the shared `TuiApp` trait but keep their existing identities. The `app/` module name refers to the shared TUI application framework, which is distinct from `widgets/file_explorer.rs` (the actual file explorer widget).

---

### Mandatory User Feedback (Revision 2)

The following changes are REQUIRED deviations from the original ADR. They override the previous "Proposed File Structure" section entirely.

#### 1. Split the View Layer -- NO monolithic framework file

The original ADR stuffed the trait, the draw skeleton, the default event loop, layout, and list rendering into a single file. The user feedback mandates a logical split:

- **`layout.rs`** -- shared 3-chunk vertical layout (explorer / status / footer). Supports single-column and multi-column variants. Contains `build_explorer_layout()`.
- **`list_view.rs`** -- list/explorer view rendering. Renders the `FileExplorerWidget` into the top chunk, including checkbox toggle and backup indicator configuration.

The `TuiApp` trait + default `run()` lives in `app/mod.rs` alongside the `App` struct (no separate trait file -- not enough code to justify its own file).

#### 2. Generic LRU Cache Module (standalone)

`preview_cache.rs` must NOT remain "unchanged". The LRU + background worker pattern is entirely generic -- the only preview-specific parts are the value type (`SessionPreview`) and the load function (`SessionPreview::load`). Extract as a generic, reusable module:

```
src/tui/lru_cache/
  mod.rs        (~30 lines)  - pub use re-exports (AsyncLruCache)
  cache.rs      (~100 lines) - AsyncLruCache<K, V>: HashMap + VecDeque, insert/get/touch/invalidate
  worker.rs     (~60 lines)  - background thread: LoadResult<K,V>, worker_loop, channel setup
```

```rust
// lru_cache/cache.rs
pub struct AsyncLruCache<K, V> {
    cache: HashMap<K, V>,
    lru_order: VecDeque<K>,
    max_size: usize,
    pending: HashSet<K>,
    request_tx: Sender<K>,
    result_rx: Receiver<LoadResult<K, V>>,
}

impl<K: Hash + Eq + Clone + Send + 'static, V: Send + 'static> AsyncLruCache<K, V> {
    pub fn new(max_size: usize, loader: impl Fn(&K) -> Option<V> + Send + 'static) -> Self { ... }
    pub fn poll(&mut self) { ... }
    pub fn get(&mut self, key: &K) -> Option<&V> { ... }
    pub fn request(&mut self, key: K) { ... }
    pub fn prefetch(&mut self, keys: &[K]) { ... }
    pub fn invalidate(&mut self, key: &K) { ... }
}
```

Preview usage is a type alias:
```rust
pub type PreviewCache = AsyncLruCache<String, SessionPreview>;
// constructed with: PreviewCache::new(20, |path| SessionPreview::load(path))
```

This separates the caching concern (LRU eviction, hit/miss) from the I/O concern (background thread, channel plumbing) AND makes the cache reusable for any future async-loaded data.

#### 3. Standalone Event Bus Module

`event.rs` must NOT remain "unchanged". It is a cross-cutting concern used by every TUI app. Rename to `event_bus.rs` to communicate its role:

```
src/tui/event_bus.rs   (~150 lines) - Event enum + EventHandler (renamed from event.rs)
```

The rename is semantic -- the file stays flat (no subdirectory needed at 146 lines) but the name `event_bus` signals "this is infrastructure shared across all TUI apps", not "events for one app".

#### 4. Unified Key Dispatch

Instead of separate `handle_search_key()`, `handle_agent_filter_key()`, `handle_help_key()` functions that each explorer calls individually in a per-mode match arm, provide a single `handle_shared_key()` dispatcher:

```rust
// app/keybindings.rs
pub enum KeyResult {
    Consumed,           // Key was handled by shared logic
    NotConsumed,        // Key was not recognized -- app should handle it
    ModeChange(Mode),   // Shared logic wants to change mode
}

pub fn handle_shared_key(
    mode: &SharedMode,
    key: KeyEvent,
    state: &mut SharedState,
) -> KeyResult { ... }
```

Each explorer's `handle_key()` calls `handle_shared_key()` first. If `KeyResult::NotConsumed`, the app handles it with app-specific logic. This eliminates the pattern of per-mode match arms calling individual handler functions.

The file `keybindings.rs` replaces the three separate files (`search.rs`, `agent_filter.rs`, `help.rs` -- now removed). Navigation keys (up/down/pgup/pgdn/home/end) are also handled here since they are identical across both apps.

#### 5. SharedState Borrow Workaround

To mitigate the borrow checker friction with trait default methods, extract the fields that shared functions need into a `SharedState` struct passed by `&mut` reference to free functions. This is NOT a god struct -- it holds only the fields needed by shared handlers (search_input, agent_filter_idx, available_agents, explorer, status_message). Each explorer owns a `SharedState` field and delegates to it.

---

### Mandatory User Feedback (Revision 4+5): file_explorer.rs Split + Preview Move

#### 6. `file_explorer.rs` Is IN SCOPE -- Must Be Split

The file `src/tui/widgets/file_explorer.rs` (1456 lines) is the center of the explorer framework and must be split. At nearly 4x the 400-line limit, it violates the single-responsibility principle by bundling a data model, a streaming file parser, sorting config, state management, rendering, and tests into one file.

**Current contents analysis:**

| Section | Lines | Concern |
|---|---|---|
| `FileItem` struct + impl + `From<SessionInfo>` | 1--81 (~81 lines) | Data model for session items |
| `SessionPreview` struct + impl (streaming parser, `styled_line_to_ratatui`, `to_ratatui_color`, `format_duration`, `parse_event_minimal`) | 82--332 (~250 lines) | Preview data loading + terminal-to-ratatui style conversion |
| `SortField` / `SortDirection` enums | 333--354 (~22 lines) | Sorting config (tiny, co-locates with state) |
| `FileExplorer` struct + impl (navigation, selection, sorting, filtering, item mutation) | 355--783 (~428 lines) | Explorer state machine |
| `FileExplorerWidget` struct + impl Widget | 784--1035 (~252 lines) | Stateless ratatui rendering |
| `format_size` helper | 1037--1052 (~16 lines) | Utility |
| Tests | 1054--1456 (~403 lines) | Unit tests for all of the above |

**Split plan -- 3 production files under `widgets/`:**

1. **`widgets/file_item.rs`** (~90 lines) -- `FileItem` struct, `FileItem::new()`, `From<SessionInfo>`, `format_size()`. The data model is a standalone value type used across the entire codebase. It has no dependency on rendering or state.

2. **`widgets/preview.rs`** (~280 lines) -- `SessionPreview` struct, `SessionPreview::load()`, `load_streaming()`, `parse_event_minimal()`, `styled_line_to_ratatui()`, `to_ratatui_color()`, `format_duration()`. Also receives `prefetch_adjacent_previews()` (moved from shared framework). This is a widget concern: it handles loading session data AND converting terminal output into ratatui rendering primitives. The `app/` module should NOT own preview rendering -- widgets should own how data is presented visually.

3. **`widgets/file_explorer.rs`** (~650 lines, accepted exception to 400-line rule) -- `FileExplorer` struct + full impl (navigation, selection, sorting, filtering, item mutation), `SortField`, `SortDirection`, `FileExplorerWidget` struct + builder methods + `impl Widget`. This file keeps the state machine and its renderer together as a single cohesive widget. The sort enums stay here because they are tightly coupled to `FileExplorer` state. The `FileExplorerWidget` rendering stays here because splitting it into a separate `explorer_widget.rs` is an artificial separation -- the state and its visual representation are the same widget. **~650 lines is explicitly accepted by the user as an exception to the 400-line rule.**

**Unit tests (~400 lines) stay inline** in `file_explorer.rs` via `#[cfg(test)]`. The project convention is: integration tests live in `tests/integration/`, unit tests stay inline in their respective files. No `widgets/tests/` subdirectory is created. With inline tests, `file_explorer.rs` totals ~1050 lines -- but only ~650 lines are production code. The user accepts this because the test code is conditionally compiled and does not contribute to production complexity.

#### 7. Move Preview Rendering to `widgets/` -- NOT `app/`

The ADR revision 2 placed `preview.rs` under `app/` (then called `explorer/`) with `prefetch_adjacent_previews()` and `extract_preview()`. The user feedback overrides this: **preview rendering is a widget concern**.

- `prefetch_adjacent_previews()` moves to `widgets/preview.rs` alongside `SessionPreview`. The function's job is to determine which adjacent items need preview loading and to issue cache requests -- this is preview logic, not app framework logic.
- `extract_preview()` (extracting the current preview from cache for the draw call) also moves to `widgets/preview.rs`.
- The `app/` directory no longer has a `preview.rs` file.

**Rationale:** The `app/` module is about the shared framework (trait, key dispatch, layout, shared state). Preview data loading and rendering is intrinsically a widget responsibility -- it answers "how do we show session data?" not "how do we structure TUI apps?".

---

### Mandatory User Feedback (Revision 6): Rename `explorer/` to `app/`, Remove ALL Backward Compatibility

#### 8. Rename `explorer/` to `app/` -- Resolve Naming Confusion

The `explorer/` module name is confusing because `widgets/file_explorer.rs` is the ACTUAL file explorer. The `explorer/` directory is NOT about exploring files -- it is a shared TUI app framework (trait, layout, key dispatch, modals, status bar).

**Solution:** Rename `explorer/` to `app/`. The existing `app.rs` (212 lines, `App` struct for terminal lifecycle) becomes `app/mod.rs` and the shared framework files live alongside it:

```
tui/app/
  mod.rs              -- App struct (terminal lifecycle, moved from app.rs) + TuiApp trait + default run() + re-exports
  shared_state.rs     -- SharedState struct + collect_agents() + constructor
  keybindings.rs      -- unified handle_shared_key() + KeyResult enum
  layout.rs           -- build_explorer_layout(), 3-chunk vertical
  list_view.rs        -- render_explorer_list() (FileExplorerWidget config)
  modals.rs           -- center_modal(), render_confirm_delete_modal()
  status_footer.rs    -- render_status_line(), render_footer()
```

The trait name changes from `ExplorerApp` to `TuiApp` since it is in the `app` module context now. `App` = terminal lifecycle struct. `TuiApp` = the trait pattern that apps follow to be runnable TUI applications.

#### 9. Remove ALL Backward Compatibility -- No Type Aliases, No Shims

The user explicitly stated: "do not keep backwards compatibility like shims or type aliases ... just go with the new structure!"

This means:
- **No type aliases** -- The original struct names `ListApp` and `CleanupApp` are retained; there are no renames.
- **Update all call sites directly** -- Every file that used the old import paths now imports the shared `TuiApp` trait and uses the original names.
- **Files stay named as-is** -- `list_app.rs` stays as `list_app.rs`. `cleanup_app.rs` stays as `cleanup_app.rs`.
- **Snapshot tests unchanged** -- Tests that referenced `ListApp::render_help_modal` continue to call `ListApp::render_help_modal`. Output stays byte-for-byte identical, only trait implementation changes.
- **Type aliases OK as intermediate step** -- During implementation, aliases can exist temporarily to keep the build green. But the FINAL committed state has zero aliases, zero shims.

---

### Proposed File Structure (Revised)

```
src/tui/
  mod.rs                       (~35 lines)  - module declarations + re-exports
  event_bus.rs                 (~150 lines) - Event enum + EventHandler (renamed from event.rs)
  ui.rs                        (unchanged)  - UI utilities (centered_rect, render_logo, render_help)

  app/
    mod.rs                     (~290 lines) - App struct (terminal lifecycle, moved from app.rs)
                                              + TuiApp trait + default run() + re-exports
    shared_state.rs            (~60 lines)  - SharedState struct + collect_agents() + constructor
    keybindings.rs             (~120 lines) - unified handle_shared_key() + KeyResult enum
    layout.rs                  (~50 lines)  - build_explorer_layout(), 3-chunk vertical
    list_view.rs               (~60 lines)  - render_explorer_list() (FileExplorerWidget config)
    modals.rs                  (~70 lines)  - center_modal(), render_confirm_delete_modal()
    status_footer.rs           (~80 lines)  - render_status_line(), render_footer()

  lru_cache/
    mod.rs                     (~30 lines)  - pub use re-exports + PreviewCache type alias
    cache.rs                   (~100 lines) - AsyncLruCache<K, V>: generic LRU with async loading
    worker.rs                  (~60 lines)  - background thread: LoadResult<K,V>, worker_loop, channels

  list_app.rs                  (~400 lines) - ListApp struct + impl TuiApp
  cleanup_app.rs               (~350 lines) - CleanupApp struct + impl TuiApp

  widgets/
    mod.rs                     (~15 lines)  - sub-module declarations + re-exports
    file_item.rs               (~90 lines)  - FileItem struct + From<SessionInfo> + format_size()
    preview.rs                 (~280 lines) - SessionPreview (load, streaming parser, style conversion,
                                              format_duration) + prefetch_adjacent_previews() +
                                              extract_preview()
    file_explorer.rs           (~650 lines) - FileExplorer struct + impl (navigation, selection,
                                              sorting, filtering) + SortField + SortDirection +
                                              FileExplorerWidget struct + builder + impl Widget
                                              (accepted exception to 400-line rule; ~1050 with
                                              inline #[cfg(test)] unit tests)
    logo.rs                    (unchanged)  - Logo widget
```

**Total: ~19 files under `src/tui/`, all under 400 lines except `file_explorer.rs` (~650 lines production, accepted exception). Most files under 120 lines. Original file names preserved. No shim files. No type aliases. No backward compatibility re-exports. No `widgets/tests/` directory.**

### How It Works

**`TuiApp` trait** (in `app/mod.rs`):
```rust
pub trait TuiApp {
    // Required accessors
    fn app(&mut self) -> &mut App;
    fn shared_state(&mut self) -> &mut SharedState;
    fn is_normal_mode(&self) -> bool;
    fn set_normal_mode(&mut self);

    // Required: app-specific behavior
    fn handle_key(&mut self, key: KeyEvent) -> Result<()>;
    fn draw(&mut self) -> Result<()>;

    // Default: shared event loop
    fn run(&mut self) -> Result<()> {
        loop {
            self.draw()?;
            match self.app().next_event()? {
                Event::Key(key) => self.handle_key(key)?,
                Event::Resize(_, _) => {}
                Event::Tick => {}
                Event::Quit => break,
            }
            if self.app().should_quit() { break; }
        }
        Ok(())
    }
}
```

**`SharedState`** (in `app/shared_state.rs`):
```rust
pub struct SharedState {
    pub explorer: FileExplorer,
    pub search_input: String,
    pub agent_filter_idx: usize,
    pub available_agents: Vec<String>,
    pub status_message: Option<String>,
    pub preview_cache: PreviewCache,
}

impl SharedState {
    pub fn new(items: Vec<FileItem>) -> Self { ... }
    pub fn apply_agent_filter(&mut self) { ... }
}
```

**Unified key dispatch** (in `app/keybindings.rs`):
```rust
pub enum KeyResult {
    Consumed,
    NotConsumed,
}

/// Dispatches keys for shared modes (Search, AgentFilter, Help, Navigation).
/// Returns Consumed if the key was handled, NotConsumed otherwise.
pub fn handle_shared_key(
    mode: &SharedMode,
    key: KeyEvent,
    state: &mut SharedState,
) -> KeyResult { ... }
```

Each explorer implements `TuiApp`. Its `handle_key()` calls `handle_shared_key()` first. If `NotConsumed`, the explorer runs app-specific logic. Each explorer has its own `Mode` enum that wraps `SharedMode` plus app-specific variants.

### file_explorer.rs Split Details

**`widgets/file_item.rs`** -- Pure data model:
```rust
pub struct FileItem {
    pub path: String,
    pub name: String,
    pub agent: String,
    pub size: u64,
    pub modified: DateTime<Local>,
    pub has_backup: bool,
}

impl FileItem { pub fn new(...) -> Self { ... } }
impl From<SessionInfo> for FileItem { ... }

pub fn format_size(bytes: u64) -> String { ... }
```

**`widgets/preview.rs`** -- Preview loading + rendering bridge:
```rust
pub struct SessionPreview {
    pub duration_secs: f64,
    pub marker_count: usize,
    pub styled_preview: Vec<StyledLine>,
}

impl SessionPreview {
    pub fn load<P: AsRef<Path>>(path: P) -> Option<Self> { ... }
    fn load_streaming<P: AsRef<Path>>(path: P) -> Option<Self> { ... }
    fn parse_event_minimal(line: &str) -> Option<(f64, EventType, Option<String>)> { ... }
    pub fn styled_line_to_ratatui(line: &StyledLine) -> Line<'static> { ... }
    fn to_ratatui_color(color: Color) -> ratatui::style::Color { ... }
    pub fn format_duration(&self) -> String { ... }
}

/// Prefetch previews for items adjacent to the current selection.
/// Issues cache requests for the selected item and its neighbors.
pub fn prefetch_adjacent_previews(
    explorer: &FileExplorer,
    cache: &mut PreviewCache,
) { ... }

/// Extract the current preview from cache for the selected item.
/// Returns None if no item is selected or preview is not yet cached.
pub fn extract_preview(
    explorer: &FileExplorer,
    cache: &mut PreviewCache,
) -> Option<&SessionPreview> { ... }
```

**`widgets/file_explorer.rs`** -- State machine + rendering widget (~650 lines production code, accepted exception to 400-line rule):
```rust
pub enum SortField { Name, Size, Date }
pub enum SortDirection { Ascending, Descending }

pub struct FileExplorer {
    items: Vec<FileItem>,
    visible_indices: Vec<usize>,
    selected: usize,
    multi_selected: HashSet<usize>,
    sort_field: SortField,
    sort_direction: SortDirection,
    agent_filter: Option<String>,
    search_filter: Option<String>,
    list_state: ListState,
    page_size: usize,
}

impl FileExplorer {
    // Constructor, navigation (up/down/page_up/page_down/home/end),
    // multi-select (toggle_select/select_all/select_none/toggle_all),
    // sorting (set_sort/apply_sort),
    // filtering (set_agent_filter/set_search_filter/clear_filters/apply_filter),
    // item mutation (remove_item/update_item_metadata/update_item_path),
    // rendering helpers (list_state/visible_items/selected_item/len/is_empty)
}

pub struct FileExplorerWidget<'a> {
    explorer: &'a mut FileExplorer,
    show_preview: bool,
    show_checkboxes: bool,
    session_preview: Option<&'a SessionPreview>,
    has_backup: bool,
}

impl<'a> FileExplorerWidget<'a> {
    pub fn new(explorer: &'a mut FileExplorer) -> Self { ... }
    pub fn show_preview(mut self, show: bool) -> Self { ... }
    pub fn show_checkboxes(mut self, show: bool) -> Self { ... }
    pub fn session_preview(mut self, preview: Option<&'a SessionPreview>) -> Self { ... }
    pub fn has_backup(mut self, has_backup: bool) -> Self { ... }
}

impl Widget for FileExplorerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) { ... }
}

// --- Unit tests stay inline ---
#[cfg(test)]
mod tests {
    // ~400 lines of unit tests for FileExplorer, FileExplorerWidget,
    // SortField, SortDirection, and rendering behavior.
    // Tests are conditionally compiled and do not contribute to
    // production line count.
}
```

The state machine and its renderer are kept together because splitting `FileExplorerWidget` into a separate `explorer_widget.rs` is an artificial separation -- they are the same widget. The renderer reads state from `FileExplorer` and cannot exist without it; there is no scenario where one changes independently of the other.

### Modal Rendering

- `render_help_modal` stays on `ListApp` (it is list-specific content).
- `render_context_menu_modal` stays on `ListApp`.
- `render_optimize_result_modal` stays on `ListApp`.
- `render_help_modal` for cleanup stays on `CleanupApp` (different keybinding content).
- `render_confirm_delete_modal` for cleanup stays on `CleanupApp` (different parameters).
- `center_modal()` utility extracted to `app/modals.rs`.

### Snapshot Test Strategy

Snapshot tests currently reference `ListApp::render_help_modal`, `ListApp::render_context_menu_modal`, `ListApp::render_optimize_result_modal`, and `OptimizeResultState`. All call sites continue to work unchanged:

1. `ListApp` keeps these as public static methods with identical signatures and identical output.
2. `OptimizeResultState` remains in `list_app.rs` and is re-exported via `mod.rs`.
3. All test imports stay the same -- no changes needed:
   - `agr::tui::list_app::ListApp` remains unchanged
   - `agr::tui::list_app::OptimizeResultState` remains unchanged
   - `agr::tui::ListApp` remains unchanged
   - `agr::tui::CleanupApp` remains unchanged
4. The rendered snapshot output is byte-for-byte identical -- import paths and method call sites do not change.

### widgets/ Re-export Strategy

The `widgets/mod.rs` must continue to re-export all public types so that existing import paths like `agr::tui::widgets::FileExplorer` still work:

```rust
// widgets/mod.rs
pub mod file_item;
pub mod preview;
pub mod file_explorer;
pub mod logo;

pub use file_item::{FileItem, format_size};
pub use preview::SessionPreview;
pub use file_explorer::{FileExplorer, FileExplorerWidget, SortDirection, SortField};
pub use logo::Logo;
```

All existing code that imports `use agr::tui::widgets::{FileExplorer, FileItem, SessionPreview, FileExplorerWidget, ...}` continues to work unchanged. Note that `FileExplorerWidget` is now re-exported from `file_explorer` instead of a separate `explorer_widget` module.

### Migration -- Call Sites That Need Updating

The original file names are preserved. Call sites are updated to use the shared `TuiApp` trait, but struct names remain unchanged. Complete list of call sites:

| File | Old import/reference | New import/reference |
|---|---|---|
| `src/commands/list.rs` | `use agr::tui::ListApp;` | `use agr::tui::ListApp;` (unchanged) |
| `src/commands/cleanup.rs` | `use agr::tui::CleanupApp;` | `use agr::tui::CleanupApp;` (unchanged) |
| `src/commands/list.rs` | `ListApp::new(...)` | `ListApp::new(...)` (unchanged) |
| `src/commands/cleanup.rs` | `CleanupApp::new(...)` | `CleanupApp::new(...)` (unchanged) |
| `tests/integration/snapshot_tui_test.rs` | `use agr::tui::list_app::ListApp;` | `use agr::tui::list_app::ListApp;` (unchanged) |
| `tests/integration/snapshot_tui_test.rs` | `use agr::tui::list_app::OptimizeResultState;` | `use agr::tui::list_app::OptimizeResultState;` (unchanged) |
| `tests/integration/snapshot_tui_test.rs` | `ListApp::render_help_modal(...)` | `ListApp::render_help_modal(...)` (unchanged) |
| `tests/integration/snapshot_tui_test.rs` | `ListApp::render_context_menu_modal(...)` | `ListApp::render_context_menu_modal(...)` (unchanged) |
| `tests/integration/snapshot_tui_test.rs` | `ListApp::render_optimize_result_modal(...)` | `ListApp::render_optimize_result_modal(...)` (unchanged) |
| `src/tui/mod.rs` | `pub mod list_app;` | `pub mod list_app;` (unchanged) |
| `src/tui/mod.rs` | `pub mod cleanup_app;` | `pub mod cleanup_app;` (unchanged) |
| `src/tui/mod.rs` | `pub use list_app::ListApp;` | `pub use list_app::ListApp;` (unchanged) |
| `src/tui/mod.rs` | `pub use cleanup_app::CleanupApp;` | `pub use cleanup_app::CleanupApp;` (unchanged) |
| `src/tui/mod.rs` | `pub mod app;` (flat file) | `pub mod app;` (directory module) |
| `src/tui/mod.rs` | `pub mod event;` | `pub mod event_bus;` |
| `src/tui/app.rs` (uses `EventHandler`) | `use crate::tui::event::EventHandler;` | `use crate::tui::event_bus::EventHandler;` |
| `src/tui/list_app.rs` | `use crate::tui::event::Event;` | `use crate::tui::event_bus::Event;` |
| `src/tui/cleanup_app.rs` | `use crate::tui::event::Event;` | `use crate::tui::event_bus::Event;` |
| `src/tui/app/mod.rs` | `use crate::tui::event::Event;` | `use crate::tui::event_bus::Event;` |

### `event.rs` -> `event_bus.rs` Migration

The rename from `event.rs` to `event_bus.rs` requires updating all imports directly -- no shim file, no re-export from old path:

- `src/tui/mod.rs` -- module declaration changes from `pub mod event;` to `pub mod event_bus;`
- `src/tui/app/mod.rs` -- uses `EventHandler` and `Event`
- `src/tui/list_app.rs` -- uses `Event`
- `src/tui/cleanup_app.rs` -- uses `Event`

All imports updated directly. The old `event.rs` file is deleted.

### Pros
- **Event loop truly shared** -- default `run()` implementation, no duplication
- **Clear naming** -- `app/` is the shared TUI app framework, `widgets/file_explorer.rs` is the actual file explorer widget, no confusion
- **No backward compat baggage** -- zero type aliases, zero shim files, clean codebase from day one
- **Original names preserved** -- no confusing renames, `ListApp` stays `ListApp`, `CleanupApp` stays `CleanupApp`
- **Minimal call site changes** -- only trait implementation and event bus import changes needed
- **Snapshot test compatibility** -- tests require zero changes to imports or method calls
- **Clean composition** -- shared handlers are free functions in `keybindings.rs`, not scattered per-mode files
- **Logical view layer split** -- layout/list_view/preview are separate concerns, not one monolith
- **Standalone infrastructure** -- event_bus and generic lru_cache are independent, reusable modules
- **Extensible** -- adding a new TUI app means implementing `TuiApp` trait
- **Type safety** -- the compiler enforces the contract
- **file_explorer.rs properly split** -- 1456-line monolith becomes 3 focused production files
- **Preview is a widget concern** -- loading and rendering preview data lives alongside the widgets that display it
- **No artificial separations** -- state machine and renderer stay together where they belong

### Cons
- **Most complex option** -- trait + accessor methods + mode composition
- **Borrow checker friction** -- mitigated by SharedState struct pattern
- **Larger diff** -- touches more code paths; trait implementation and event_bus imports must be updated
- **Only 2 apps** -- trait may be slight over-engineering
- **More files** -- 19 files vs. current 10, but all are small and focused
- **One file exceeds 400-line rule** -- `file_explorer.rs` at ~650 lines production code (~1050 with inline tests), explicitly accepted by the user as a pragmatic exception

### Estimated Result
- `list_app.rs`: ~400 lines (down from 1352) -- `ListApp` struct, implements `TuiApp` trait
- `cleanup_app.rs`: ~350 lines (down from 879) -- `CleanupApp` struct, implements `TuiApp` trait
- `app/mod.rs`: ~290 lines (App struct from old app.rs + TuiApp trait + default run + re-exports)
- `app/keybindings.rs`: ~120 lines (unified key dispatch)
- All other `app/` files: under 80 lines each
- `lru_cache/`: 3 files, all under 100 lines (generic `AsyncLruCache<K, V>`)
- `event_bus.rs`: ~150 lines (renamed from event.rs)
- `widgets/file_item.rs`: ~90 lines (data model extracted)
- `widgets/preview.rs`: ~280 lines (SessionPreview + prefetch + extract)
- `widgets/file_explorer.rs`: ~650 lines production (state + rendering + sort enums, accepted exception to 400-line rule; ~1050 with inline `#[cfg(test)]` unit tests)
- All snapshot tests pass with byte-for-byte identical output (import paths and call sites unchanged)
- **All files under 400 lines except `file_explorer.rs` (~650 lines production, user-accepted exception)**
- Zero shim files
- Zero type aliases
- Original file and struct names preserved
- No `widgets/tests/` directory -- unit tests stay inline per project convention

---

## Rejected Options

### Option A: Flat Component Extraction
Simple, low risk, but leaves event loop and state fields duplicated. Does not eliminate structural duplication in `draw()` and `handle_key()`.

### Option B: Shared AppState Struct
Good middle ground, but `SharedAppState` risks becoming a god struct. The `self.shared.explorer` indirection is less ergonomic than the trait accessor pattern.

---

## Comparison Matrix

| Criterion | Option A (Flat) | Option B (Shared State) | **Option C (TuiApp Trait)** |
|---|---|---|---|
| **Eliminates code duplication** | High (logic) | Highest (logic + state) | **Highest (logic + state + event loop)** |
| **Simplicity** | Highest | Medium | **Medium (accessor pattern is well-known)** |
| **Snapshot test safety** | Highest | Medium | **High (direct call site updates, identical output)** |
| **Incremental delivery** | Easiest | Medium | **Medium (extract from list_app first)** |
| **Future extensibility** | Low | Medium | **Highest** |
| **Risk of regression** | Lowest | Medium | **Medium** |
| **Domain naming clarity** | Low | Medium | **Highest** |
| **Max file size** | ~80 lines | ~200 lines | **~650 lines (file_explorer.rs, accepted exception)** |
| **Borrow checker friction** | Low | Low-Medium | **Medium (mitigated by SharedState)** |
| **View layer separation** | N/A | N/A | **High (layout/list_view/preview split)** |
| **Infrastructure isolation** | Low | Low | **High (event_bus + preview_cache standalone)** |
| **Widget decomposition** | N/A (out of scope) | N/A (out of scope) | **High (file_explorer split into 3 production files)** |
