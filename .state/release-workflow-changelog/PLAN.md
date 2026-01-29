# Plan: Release Workflow and Changelog Automation

References: ADR.md

## Open Questions

Implementation challenges to solve (architect identifies, implementer resolves):

1. git-cliff installation method in CI - use cargo install or pre-built binary?
2. Exact changelog commit message format - should it include version number?

## Stages

### Stage 1: git-cliff Configuration

Goal: Create cliff.toml for conventional commit parsing

- [x] Create `cliff.toml` in repository root
- [x] Configure commit groups: feat, fix, refactor, chore, docs, perf, test
- [x] Set changelog header template
- [x] Configure version tag pattern matching

Files: `cliff.toml`

Considerations:
- Use conventional commits grouping (feat -> Features, fix -> Bug Fixes, etc.)
- Include commit scope in output if present
- Link to GitHub compare URLs between versions
- Test locally with `git-cliff --unreleased` before committing

### Stage 2: Create release.yml Workflow

Goal: New workflow triggered by version tags that builds releases

- [x] Create `.github/workflows/release.yml`
- [x] Trigger on `v*` tag push only
- [x] Add git-cliff installation step
- [x] Generate CHANGELOG.md from all commits up to tag
- [x] Auto-commit CHANGELOG.md to main branch
- [x] Build release binaries for all 3 platforms
- [x] Create GitHub Release with binaries attached
- [x] Set permissions for contents: write

Files: `.github/workflows/release.yml`

Considerations:
- Follow existing `docs.yml` pattern for auto-commit setup
- Use matrix build for platforms: linux-x86_64, macos-x86_64, macos-arm64
- Binary naming: `asr-<platform>` (e.g., `asr-linux-x86_64`)
- Use `[skip ci]` in changelog commit to avoid retriggering CI
- GitHub Release body can reference CHANGELOG.md or include release notes

### Stage 3: Modify ci.yml (Remove Release Job)

Goal: Clean separation - ci.yml for checks only

- [x] Remove the `release` job (lines 222-255)
- [x] Remove artifact upload from `build` job (lines 40-44)
- [x] Verify remaining jobs: build, unit-tests, coverage, e2e-tests, snapshot-tests, lint

Files: `.github/workflows/ci.yml`

Considerations:
- Build job still needed for cache warming and as dependency for other jobs
- Keep `Upload binary` in build job if needed for e2e tests (verify dependency)
- Ensure no jobs depend on removed release job

### Stage 4: Update Maintainer Role

Goal: Document release process for maintainers

- [x] Add "Release Process" section to maintainer.md
- [x] Document tagging procedure: `git tag v0.1.0 && git push origin v0.1.0`
- [x] Document when to bump major/minor/patch
- [x] Add "End of Cycle Tasks" checklist
- [x] Note: CHANGELOG.md is auto-generated, manual edits optional

Files: `.claude/skills/roles/references/maintainer.md`

Considerations:
- Keep instructions concise and actionable
- Include example commands
- Reference semver briefly

### Stage 5: Populate INDEX.md Completed Work

Goal: Add historical context for all existing state directories

- [x] Add "Completed Work" section to INDEX.md
- [x] List all 11 state directories with one-sentence descriptions
- [x] Format: `| directory-name | Brief description |`

Files: `.state/INDEX.md`

State directories to document:
1. `chore-improve-orchestrator-sdlc` - Orchestrator role instructions improvements
2. `chore-improve-roles-sdlc` - SDLC role system with ADR/PLAN separation
3. `feature-optimize-ui-improvements` - Renamed transform to optimize, UI polish
4. `feature-two-phase-review-workflow` - Two-phase code review process
5. `file-explorer-transform` - TUI context menu and transform integration
6. `fix-player-scroll-region-bug` - Terminal scroll region (DECSTBM) support
7. `fix-transform-backup-bugs` - Backup file handling and atomic operations
8. `refactor-asciicast-module` - Asciicast module restructuring
9. `refactor-terminal-module-cleanup` - Terminal module re-export chain cleanup
10. `silence-removal` - Silence removal transform implementation
11. `release-workflow-changelog` - (this work) Release workflow and changelog

Considerations:
- Read each ADR to write accurate descriptions
- Keep descriptions to one sentence
- This provides quick overview without opening each ADR

## Dependencies

What must be done before what:

- Stage 2 depends on Stage 1 (release.yml needs cliff.toml to exist)
- Stage 3 can run in parallel with Stage 2
- Stage 4 can run in parallel with Stage 2 and 3
- Stage 5 can run in parallel with all stages

Recommended order: 1 -> 2 -> 3 -> 4 -> 5

## Progress

Updated by implementer as work progresses.

| Stage | Status | Notes |
|-------|--------|-------|
| 1 | complete | cliff.toml created with conventional commits grouping |
| 2 | complete | release.yml with changelog generation, multi-platform builds, GitHub Release |
| 3 | complete | Removed release job and artifact upload from ci.yml |
| 4 | complete | Added Release Process section with user approval requirement |
| 5 | complete | Added Completed Work table with 11 state directories |
