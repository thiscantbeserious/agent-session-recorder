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
- Configurable filename templates for recordings (PR #83)
- Play command for direct recording playback (PR #81)
- Release workflow and changelog automation (PR #75)
- Terminal scroll region support (PR #73)

## Completed Work

Historical context for all state directories:

| Directory | Description |
|-----------|-------------|
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
