# State Index

## Where to Find Things

| What | Where |
|------|-------|
| Completed work | `gh pr list --state merged` |
| Current branch | `git branch --show-current` |
| Open PRs | `gh pr list` |
| Technical decisions | `.state/decisions.md` |
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
- E2E test refactor into category files (PR #17 merged)
- Shell wrapper & prompt fixes (PR #16 merged)
