# State Index

## Where to Find Things

| What | Where |
|------|-------|
| Completed work | `gh pr list --state merged` |
| Current branch | `git branch --show-current` |
| Open PRs | `gh pr list` |
| Technical decisions | `.state/PROJECT_DECISIONS.md` |
| Templates | `.state-templates/` |
| Archives | `.archive/state/` |
| Architecture docs | `architecture/` |

## Quick Commands

```bash
# See what's been done
gh pr list --state merged --limit 20

# See what's in progress
gh pr list

# Current context
git branch --show-current
git log --oneline -5
```

## Active Work

<!-- Keep this minimal - just what's currently being worked on -->

**Current focus:** None (phase complete)

**Recently completed:**
- Refactor analyze command: parallel LLM analysis with content extraction pipeline, multi-backend support, and aggressive noise reduction (PR #112)
- Final checks: Miri + ASan/LSan CI jobs for memory safety (PR #109)
- Miri CI job with label-based trigger (PR #108)
- count_digits floating-point precision fix (PR #107)
- CI optimization with caching, path filters, and E2E test fixes (PR #104)
- Clipboard copy for recordings (PR #103)
- Aggressive shell minification for RC file embedding (PR #90)
- Native player modular refactor with state guards (PR #88)
- Configurable filename templates for recordings (PR #83)
- Play command for direct recording playback (PR #81)

## Completed Work

Historical context for all state directories:

| Directory | Description |
|-----------|-------------|
| refactor-analyze-command | Parallel LLM analysis with content extraction, multi-backend support, noise reduction |
| feature-clipboard-copy | Clipboard copy for recordings - CLI and TUI with cross-platform support |
| enhance-shell-minification | Aggressive shell script minification for compact RC file embedding |
| refactor-native-player-modules | Native player modular refactor with state guards and bug fixes |
| feature-naming-template | Configurable filename templates for recordings |
| feature-play-command | Play command for direct recording playback |
| silence-removal | Silence/pause removal for recordings |
| fix-player-scroll-region-bug | Terminal scroll region support in player |
| refactor-terminal-module-cleanup | Terminal module code organization |
| refactor-asciicast-module | Asciicast types extraction and Transform trait |
| fix-transform-backup-bugs | Backup reliability fixes for transform ops |
| file-explorer-transform | Transform integration in TUI file explorer |
| feature-two-phase-review-workflow | Two-phase SDLC review process |
| feature-optimize-ui-improvements | TUI polish: rename transform to optimize, contrast fixes |
| chore-improve-roles-sdlc | ADR/PLAN separation pattern |
| chore-improve-orchestrator-sdlc | Orchestrator documentation and boundaries |
| release-workflow-changelog | Release workflow and changelog automation |
