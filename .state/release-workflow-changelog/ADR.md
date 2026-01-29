# ADR: Release Workflow and Changelog Automation

## Status
Accepted

## Context

The project currently has no formal release process. There are:
- No version tags
- No CHANGELOG.md
- Release artifacts are built on every main push (wasteful)
- No documented release process for maintainers

The project is mature enough to benefit from proper versioning and release management. Users and contributors need visibility into what changed between versions.

### Forces at Play

- **Automation preference**: Manual changelog maintenance is tedious and error-prone
- **CI capabilities**: Existing `docs.yml` demonstrates write access via GITHUB_TOKEN works
- **Maintainer control**: Need ability to edit changelog before release if needed
- **LLM context**: State directories provide valuable context and should be preserved

## Options Considered

### Option 1: Local git-cliff (maintainer runs manually)

- Pros: Simple, full maintainer control, no CI complexity
- Cons: Manual step before every release, easy to forget

### Option 2: CI-generated CHANGELOG (automated on tag push)

- Pros: Fully automated, consistent, no manual steps
- Cons: Slightly more CI complexity, maintainer edits require amending after generation

## Decision

**Option 2: CI-generated CHANGELOG with automation**

The release workflow will:
1. Trigger on `v*` tag push
2. Run git-cliff to generate/update CHANGELOG.md
3. Auto-commit the changelog to main
4. Build release artifacts for all platforms
5. Create GitHub Release with artifacts attached

This follows the existing `docs.yml` pattern for auto-commits. Maintainer can still edit CHANGELOG.md manually before or after tagging if refinement is needed.

### Versioning

- Start at `v0.1.0`
- Follow semver: `vMAJOR.MINOR.PATCH`
- Maintainer decides when to tag based on accumulated changes

### CI Architecture

**ci.yml (checks only)**:
- Build, test, lint, coverage, snapshot tests
- Runs on: push to main, PRs to main
- No release artifacts (clean separation)

**release.yml (releases only)**:
- Trigger: `v*` tag push
- Generate CHANGELOG.md via git-cliff
- Build release binaries (linux-x86_64, macos-x86_64, macos-arm64)
- Create GitHub Release with attached binaries

### git-cliff Configuration

Use conventional commits format:
- `feat:` -> Features
- `fix:` -> Bug Fixes
- `refactor:` -> Refactoring
- `chore:` -> Maintenance
- `docs:` -> Documentation

### INDEX.md Updates

Add "Completed Work" section listing all state directories with one-sentence descriptions. This provides quick context for future work without needing to open each ADR.

## Consequences

### Positive
- Automated changelog generation on every release
- Clean CI separation (checks vs releases)
- Proper versioning with semver
- GitHub Releases with downloadable binaries
- State directories preserved for LLM context
- Quick overview of past work in INDEX.md

### Negative
- git-cliff is an external dependency (installed in CI)
- Changelog commits create slightly more git history
- Must use conventional commit format for best results

### Follow-ups
- Consider adding changelog preview to PRs (future enhancement)
- Consider automated version bumping (future enhancement)

## Decision History

1. User chose CI-generated changelog (Option B) over local generation for automation preference
2. User confirmed clean CI separation: ci.yml for checks only, release.yml for releases
3. State directories are kept as LLM context (not archived) - per requirements
4. CHANGELOG.md auto-committed to main after tag push, following existing docs.yml pattern
