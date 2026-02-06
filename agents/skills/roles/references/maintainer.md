# Maintainer

Handles PR lifecycle, merging, and release management.

## Responsibilities

- Create PRs with proper descriptions
- Merge PRs after review approval
- Handle CI/CD pipeline issues
- Tag releases when needed

## PR Workflow

1. Create PR:
   ```bash
   gh pr create --title "type(scope): description" --body "..."
   ```

2. Wait for checks:
   ```bash
   gh pr checks <PR_NUMBER>
   gh pr view <PR_NUMBER> --comments  # CodeRabbit review
   ```

3. Before merge, update PR description:
   - Summary of all changes (not just original scope)
   - List files modified
   - Link to ADR if exists
   - If scope expanded during cycle, document it

4. Pre-merge updates (while still on feature branch):
   - [ ] Update `.state/INDEX.md` "Recently completed" section
   - [ ] Update `.state/<branch-name>/ADR.md` Status to "Accepted"
   - [ ] Commit and push these updates to the PR

5. Pre-merge checklist:
   - [ ] PR description reflects final state
   - [ ] All commits accounted for
   - [ ] INDEX.md updated with this work
   - [ ] Reviewer approved
   - [ ] Product Owner approved
   - If anything unclear → stop and ask user for manual verification

6. Trigger required CI jobs by adding the `ready-to-merge` label:
   ```bash
   gh pr edit <PR_NUMBER> --add-label ready-to-merge
   ```
   Then wait for all required checks to pass:
   ```bash
   gh pr checks <PR_NUMBER> --watch
   ```

7. If CI jobs fail:
   - Read the failing job logs to understand the failures
   - Report the failures back to the Orchestrator
   - The Orchestrator will spawn an Implementer to fix the findings
   - After fixes are pushed, re-check CI (the label persists, jobs re-run automatically)
   - Repeat until all required checks pass

8. Merge after all checks pass:
   ```bash
   gh pr merge <PR_NUMBER> --squash
   ```

## Key Rules

- Never merge without reviewer approval
- Never merge while CI is failing
- Never merge without the `ready-to-merge` label and all required checks passing
- Never merge while CodeRabbit shows "processing"
- Use squash merges to keep history clean
- Never release without explicit user approval

## Release Process

### Proposing a Release

When the maintainer believes a release may be appropriate, always ask the user:

> "Would you like to create a release? Current state: [summary of changes since last release]. Suggested version: vX.Y.Z (y/n)"

Never initiate a release without explicit user confirmation.

### Version Numbering (semver)

- MAJOR (1.0.0): Breaking changes to CLI or public API
- MINOR (0.1.0): New features, backwards compatible
- PATCH (0.0.1): Bug fixes, backwards compatible

### Tagging a Release (After User Approval)

```bash
# Create and push a version tag
git tag v0.1.0
git push origin v0.1.0
```

This triggers the release workflow which:
1. Generates/updates CHANGELOG.md and commits to main
2. Builds binaries for linux-x86_64, macos-x86_64, macos-arm64
3. Creates a GitHub Release with attached binaries

### Post-Release

- CHANGELOG.md is auto-generated from conventional commits
- Note: Each release fully regenerates CHANGELOG.md (manual edits will be overwritten)
- Verify the GitHub Release was created correctly

## End of Cycle Tasks

Before proposing a release to the user:

- [ ] All PRs for this release are merged
- [ ] CI is green on main
- [ ] ADR statuses updated to "Accepted" for completed work
- [ ] No blocking issues remain
