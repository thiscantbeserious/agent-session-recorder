# ADR: TUI Context Menu and Transform Integration

## Status
Accepted

## Context

The silence removal transform (`agr transform --remove-silence`) exists only as a CLI command. Users browsing recordings in the TUI file explorer must exit, run the transform manually, then re-enter the TUI. This disrupts the workflow.

Additionally, the current Enter key behavior (immediate playback) doesn't scale as more actions are added. A context menu provides better discoverability and room for future actions.

### Forces at Play

1. **Usability**: Users want quick access to transforms without leaving the TUI
2. **Discoverability**: New users need to discover available actions
3. **Power user efficiency**: Experienced users want keyboard shortcuts
4. **Future extensibility**: Spinner detection (research/algorithm_for_asciicast_cutting_and_compression.md, Section 5) will be added later
5. **Data safety**: Transforms modify files; users need restore capability
6. **Consistency**: Follow existing modal patterns (Help, ConfirmDelete)

## Options Considered

### Option 1: Single Context Menu Modal
Add `Mode::ContextMenu` that renders all actions in one modal. Simple, follows existing patterns.

- Pros: Minimal code, consistent with existing modals
- Cons: No nested menus if transforms need sub-options later

### Option 2: Trait-based Action System
Create an `Action` trait for extensible, declarative actions.

- Pros: Highly extensible, actions are self-contained
- Cons: Overkill for 4 actions, requires significant refactoring

### Option 3: Incremental Modes with Direct Shortcuts
Context menu for discoverability (Enter), direct shortcuts for power users (p/t/d/m in Normal mode). Modal feedback for transform results.

- Pros: Best of both worlds, matches requirements exactly, incremental
- Cons: More keyboard handling surface area

## Decision

**Option 3: Incremental Modes with Direct Shortcuts**

This approach:
- Provides discoverability through the context menu (Enter key)
- Maintains efficiency for power users (direct shortcuts bypass menu)
- Uses modal dialogs for feedback (consistent with existing patterns)
- Abstracts transforms behind a single "Transform" action (future-proof for spinner detection)

### Key Design Decisions

1. **Transform Feedback**: Modal dialog requiring dismissal (shows time saved, original duration)

2. **Backup Strategy**: Preserve original only - if `.bak` exists, don't overwrite it. This ensures users can always restore to the true original state, even after multiple transforms.

3. **Backup Display in TUI**:
   - File list (left panel): Filter out `.bak` files - they should NOT appear in the list
   - Preview panel (right panel): Show "Backup available" indicator when a `.bak` file exists for the selected recording
   - This keeps the list uncluttered while informing users that restore is available

4. **Context Menu Options**: Play, Transform, Delete, Add Marker, Restore (5 items total). Arrow key + Enter navigation only within the menu. Single-key shortcuts (`p`, `t`, `d`, `m`, `r`) work from Normal mode to bypass the menu entirely.

5. **Transform Abstraction**: The TUI exposes a single "Transform" action that applies all available transforms. Currently this is only silence removal, but when spinner detection is added, it will be included automatically without changing the UI.

## Consequences

### What becomes easier
- Applying transforms without leaving TUI
- Discovering available actions through context menu
- Restoring original files after transform
- Adding new transforms (spinner detection) without UI changes

### What becomes harder
- Nothing significant; complexity is managed through staged implementation

### Critical: Preserve Existing Functionality

The following existing features MUST continue to work after implementation:

1. **`cleanup` command** - Deletes old recordings based on age/size criteria. Transform backups (`.bak` files) should be handled appropriately (either included in cleanup or explicitly excluded).

2. **`ls` command** - Lists recordings. Should not be affected, but verify file enumeration still works correctly after transforms create `.bak` files.

3. **Preview functionality** - The file explorer's preview panel shows duration, markers, and terminal snapshot. After a transform modifies a file, the preview must refresh correctly to show the new duration.

Each stage must include regression testing for these features where relevant.

### Testing Strategy

**Snapshot Testing**: All UI stages must include snapshot tests using `insta` crate to detect visual regressions:
- Context menu modal rendering
- Transform result modal rendering
- Preview panel with backup indicator
- Help modal updates

This ensures UI changes are intentional and reviewable in PRs.

**Round-Trip Integrity Test**: Critical test to verify backup/restore preserves data integrity:
1. Take a sample asciicast file
2. Apply transform (creates backup, modifies file)
3. Restore from backup
4. Apply transform again (must NOT create new backup - one already exists)
5. Restore again
6. Compare final file with original - **must be byte-for-byte identical**

This test validates that the "backup original only" strategy correctly preserves the true original across multiple transform/restore cycles.

**CI Integration**: Snapshot tests must be verified in CI to prevent unreviewed UI changes:
- Add separate `snapshot-tests` job to CI pipeline (not integrated into existing test job)
- Job runs `cargo insta test --check` which fails if snapshots don't match committed versions
- Job runs in parallel with other test jobs for faster CI
- Developers must run `cargo insta review` locally to approve changes before committing

### Follow-ups to scope for later
- Per-transform customization (custom thresholds)
- Batch transforms on multiple selected files
- Transform preview before applying
- Spinner detection integration (research document Section 5)
- Cleanup command awareness of `.bak` files (decide: include or exclude)

## Decision History

Decisions made during design:

1. User selected modal dialog (Option A) for transform feedback over status line or auto-dismiss
2. User selected "backup original only" - don't overwrite existing .bak files to preserve true original
3. User selected arrow key + Enter navigation only for context menu (Option B), with direct shortcuts still available from Normal mode
4. Research document analysis confirmed transform abstraction approach - single "Transform" action that applies all enabled transforms
5. Commit research/ folder as part of Stage 1 to preserve algorithm documentation
6. User emphasized preserving existing functionality: cleanup command, ls command, and preview functionality must not break
7. User clarified backup display: filter `.bak` from file list, show "Backup available" indicator in preview panel
8. Context menu includes Restore option (synced with Product Owner requirements - 23 acceptance criteria total)
9. Snapshot tests required for all UI stages using `insta` crate for visual regression detection
10. Round-trip integrity test required: transform -> restore -> transform -> restore must yield byte-identical original file
11. CI integration for snapshot tests: separate `snapshot-tests` job running `cargo insta test --check`
