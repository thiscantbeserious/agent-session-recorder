# Plan: Reduce Cognitive Complexity — tui/list_app

References: [ADR.md](ADR.md) | [REQUIREMENTS.md](REQUIREMENTS.md)

## Status: Completed

## Stages

### Stage 1: Baseline verification
- [x] `cargo test` passes before changes
- [x] `cargo clippy` clean before changes

### Stage 2: Extract helper functions
#### 2a. `ListApp::draw()` — score 80
- [x] `render_status_line_for_mode()` — free function extracting 11-arm mode match for status line rendering
- [x] `build_rename_status_spans()` — free function extracting deeply nested rename input span construction
- [x] `footer_text_for_mode()` — free function extracting 12-arm mode match returning static footer text
- [x] `render_modal_overlays()` — free function extracting 8-arm mode match for modal rendering

#### 2b. `ListApp::handle_mouse()` — score 48
- [x] `handle_normal_mouse()` — method extracting Normal mode mouse handling with click-to-index
- [x] `handle_context_menu_mouse()` — method extracting ContextMenu mode mouse with hit testing
- [x] `handle_confirm_modal_mouse()` — method extracting confirm dialog mouse handling

#### 2c. `handle_rename_input_key()` — score 18
- [x] `handle_rename_backspace()` — method extracting select-all clear vs char-boundary deletion
- [x] `handle_rename_delete()` — method extracting forward deletion logic
- [x] `handle_rename_char_input()` — method extracting char validation and insert-at-cursor

#### 2d. `render_context_menu_modal()` — score 18
- [x] `build_menu_item_label()` — free function extracting label formatting with shortcut/disabled hints
- [x] `menu_item_style()` — free function extracting selected/disabled/normal style selection

#### 2e. `handle_normal_key()` — score 17
- [x] `redirect_if_locked()` — method deduplicating 6 identical locked-check-and-redirect blocks

### Stage 3: Regression verification
- [x] `cargo test` passes after changes
- [x] `cargo clippy` clean after changes
- [x] `cargo fmt` applied

### Stage 4: Review
- [x] Pair review: PASS
- [x] Internal review: APPROVE

## Files Modified
- `src/tui/list_app.rs` — extracted 12 helpers (4 free functions + 8 methods) from 5 flagged functions, reducing combined complexity from 181 to well below 15 each

## Extracted Functions
| Original Function | Score | Extracted Helpers | New Score |
|---|---|---|---|
| `ListApp::draw()` | 80 | `render_status_line_for_mode()`, `build_rename_status_spans()`, `footer_text_for_mode()`, `render_modal_overlays()` | < 15 |
| `ListApp::handle_mouse()` | 48 | `handle_normal_mouse()`, `handle_context_menu_mouse()`, `handle_confirm_modal_mouse()` | < 15 |
| `handle_rename_input_key()` | 18 | `handle_rename_backspace()`, `handle_rename_delete()`, `handle_rename_char_input()` | < 15 |
| `render_context_menu_modal()` | 18 | `build_menu_item_label()`, `menu_item_style()` | < 15 |
| `handle_normal_key()` | 17 | `redirect_if_locked()` | < 15 |
