# Requirements: Modularize list_app and cleanup_app into shared TUI components

## Problem Statement
`src/tui/list_app.rs` (1352 lines) and `src/tui/cleanup_app.rs` (879 lines) contain heavily duplicated code:

- Nearly identical `Mode` enums (Normal, Search, AgentFilter, Help, ConfirmDelete)
- Nearly identical event loops (`run()`)
- Nearly identical `draw()` layouts (3-chunk vertical: explorer + status + footer)
- Nearly identical input handlers: `handle_search_key`, `handle_agent_filter_key`, `handle_help_key`, `handle_confirm_delete_key`
- Nearly identical helper methods: `apply_agent_filter`, `prefetch_adjacent_previews`
- Nearly identical modal rendering patterns (center modal, clear, paragraph in bordered block)
- Identical agent collection logic in constructors

Both files exceed the 400-line limit. The duplication makes maintenance error-prone — a fix in one must be manually replicated in the other.

## Desired Outcome
Individual functionality extracted into their own component files. Use `list_app` as the primary/reference implementation — extract shared components from it first, then have `cleanup_app` consume them.

Each distinct concern gets its own file (e.g., search input handling, modal rendering, agent filtering — each in a dedicated component file). Both app files should slim down to orchestrators that compose these components.

## Scope
### In Scope
- Extract shared input handling (search, agent filter, help, navigation)
- Extract shared modal rendering (help modal, confirm delete modal, modal centering/clearing)
- Extract shared state (mode enum, agent filter state, search state, preview cache management)
- Extract shared event loop pattern
- Reduce both app files to ~400 lines or under
- Keep each app's unique logic in its own file

### Out of Scope
- Changing any visual behavior or keybindings
- Adding new features to either app
- Refactoring the FileExplorer widget itself
- Changing the App/Event infrastructure

## Acceptance Criteria
- [ ] No file exceeds ~400 lines
- [ ] Zero code duplication between list_app and cleanup_app for shared patterns
- [ ] All existing tests pass (`cargo test`)
- [ ] All snapshot tests pass (`cargo insta test`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] Both apps behave identically to before (pure refactoring)
- [ ] All UI snapshot tests pass unchanged (`cargo insta test --check`)

## Constraints
- Pure refactoring — zero behavior changes
- Must not break snapshot tests (modal rendering must produce identical output)
- Shared modules should be generic enough for both apps but not over-engineered

## Context
- Both apps use the `FileExplorer` widget from `tui/widgets/`
- Both apps use `PreviewCache` for async preview loading
- Both apps use `App` for terminal handling (raw mode, alternate screen, event loop)
- The `tui/` module already has `app.rs`, `event.rs`, `ui.rs`, `widgets/` — new shared modules fit naturally here
- Coding principles: ~400 lines/file, ~20 lines/function, max 3 nesting levels

---
**Sign-off:** Approved by user
