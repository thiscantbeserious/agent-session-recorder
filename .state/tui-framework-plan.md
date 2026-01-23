# TUI Framework Plan

## Goal

Add a TUI (Text User Interface) framework to AGR that enables:
1. Dynamic terminal resize handling (like Claude Code)
2. Interactive file explorer for `list` and `cleanup` commands
3. Rich UI components for future features

**TUI is the primary experience. Small fallback for piped usage (safety net).**

## Simple Logic

```
list/cleanup:
    TUI always (default experience)
    fallback to text if piped (safety net, not primary use case)
```

No flags needed. TUI is the primary experience.

## Crate Selection

**`ratatui`** + **`crossterm`**
- Active maintenance, cross-platform
- Handles resize events (`SIGWINCH`)
- Built-in widgets (lists, tables)

## Implementation Phases

### Phase 1: Foundation
**Branch:** `feat/tui-foundation`
**Scope:** Add dependencies, create TUI module, dynamic logo in help

Tasks:
1. Add `ratatui` and `crossterm` dependencies
2. Create `src/tui/` module skeleton
3. Logo widget with dynamic REC line (resizes with terminal)
4. `--help` uses TUI when TTY, static when piped

**Manual testing:**
- [ ] `agr --help` - logo resizes when terminal resized
- [ ] `agr --help | cat` - static output, no TUI
- [ ] Resize terminal while help displayed - REC line updates

**Exit criteria:**
- Dynamic logo works
- Piped output unchanged
- All existing tests pass
- **Manual verification before merge**

---

### Phase 2: File Explorer Widget
**Branch:** `feat/tui-file-explorer`
**Scope:** Create reusable file explorer component

Tasks:
1. Create `src/tui/widgets/file_explorer.rs`
2. Features:
   - Arrow keys navigation (↑/↓)
   - Page up/down, Home/End
   - Multi-select with space
   - Sort by date/size/name
   - Filter by agent
   - Preview panel (file info)
3. Footer shows keyboard shortcuts

**Manual testing:**
- [ ] Navigate up/down with arrows
- [ ] Page through long lists
- [ ] Multi-select works
- [ ] Sort toggles work
- [ ] Resize terminal - layout adapts

**Exit criteria:**
- Widget works standalone
- Unit tests for navigation, selection
- **Manual verification before merge**

---

### Phase 3: `list` with File Explorer
**Branch:** `feat/tui-list`
**Scope:** `agr list` uses file explorer (TUI primary)

Behavior:
- **Primary (TUI):** Interactive file explorer
  - `/` → search (fuzzy filter by filename)
  - `f` → filter by agent type (dropdown/toggle)
  - Enter → play session (`asciinema play`)
  - `d` → delete (with confirmation)
  - `m` → add marker
  - `q` → quit
  - `?` → help
- **Fallback (piped):** Simple text list (safety net)

**Manual testing:**
- [ ] `agr list` - file explorer opens
- [ ] `/` opens search, typing filters list
- [ ] `f` shows agent filter, can toggle agents
- [ ] Enter plays session
- [ ] `d` deletes with confirmation
- [ ] `q` quits cleanly
- [ ] Resize terminal - layout adapts
- [ ] `agr list | cat` - fallback text works

**Exit criteria:**
- TUI explorer with search + filter works
- Fallback doesn't crash
- All existing tests pass
- **Manual verification before merge**

---

### Phase 4: `cleanup` with File Explorer
**Branch:** `feat/tui-cleanup`
**Scope:** `agr cleanup` uses file explorer (TUI primary)

Behavior:
- **Primary (TUI):** File explorer with checkbox multi-select
  - Space → toggle checkbox on current item
  - `a` → select all / deselect all
  - `g` → glob select (e.g., `claude/*.cast`, `*2024*`)
  - `/` → search filter
  - `f` → filter by agent type
  - Shows total storage to be freed
  - Enter → confirm delete selected
  - Esc → cancel
- **Fallback (piped):** Simple prompt-based cleanup (safety net)

**Manual testing:**
- [ ] `agr cleanup` - explorer with checkboxes
- [ ] Space toggles checkbox on item
- [ ] `a` selects/deselects all
- [ ] `g` opens glob input, pattern selects matching files
- [ ] `/` filters list by search
- [ ] `f` filters by agent
- [ ] Selected items show total size to free (updates live)
- [ ] Enter deletes selected with confirmation
- [ ] Esc cancels without deleting
- [ ] Resize terminal - layout adapts

**Exit criteria:**
- TUI explorer with checkboxes + glob select works
- Fallback doesn't crash
- All existing tests pass
- **Manual verification before merge**

---

### Phase 5: Recording Status (Future)
- Live recording indicator
- Duration counter
- Storage meter

## Architecture

```
src/
├── tui/
│   ├── mod.rs
│   ├── app.rs              # Event loop, state
│   ├── event.rs            # Key/resize handling
│   ├── ui.rs               # Layout
│   └── widgets/
│       ├── mod.rs
│       ├── logo.rs         # Dynamic REC line
│       └── file_explorer.rs
├── commands/
│   ├── list.rs             # TTY → explorer, else → text
│   ├── cleanup.rs          # TTY → explorer, else → prompts
│   └── ...
└── main.rs
```

## Fallback (Safety Net)

**If piped, don't crash - output simple text:**
- `agr list | cat` - prints text list
- `agr cleanup | cat` - prints text prompts

Not the primary use case, just ensures pipes don't break.

## Dependencies

```toml
ratatui = "0.29"
crossterm = "0.28"
```

## References

- [ratatui docs](https://docs.rs/ratatui)
- [ratatui examples](https://github.com/ratatui-org/ratatui/tree/main/examples)
