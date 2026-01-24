# Git Workflow

## Branch Protection

**`main` is protected** - you cannot push directly to main. All changes must go through a PR.

## Feature Branch Workflow

```bash
# Create feature branch
git checkout -b feature/phase1-task-name

# After implementation
git add -A
git commit -m "feat(scope): description"
git push -u origin feature/phase1-task-name

# Create PR
gh pr create --title "feat(scope): description"

# After review & merge
git checkout main
git pull
```

## Pull Request & CI

**MANDATORY: Wait for CI AND CodeRabbit to pass before merging!**

### Creating PRs

1. Create PR: `gh pr create --title "feat(scope): description"`
2. Wait for all checks to complete:
   ```bash
   gh pr checks <PR_NUMBER>   # Check CI status
   gh pr view <PR_NUMBER> --comments   # Check CodeRabbit review
   ```

### CI Pipeline Stages (must ALL pass)

- **Build** - compilation on ubuntu + macos
- **Unit Tests** - cargo test on both OS
- **E2E Tests** - integration tests (macOS only)
- **Lint** - cargo fmt + clippy

### CodeRabbit Review (must complete)

- Wait for CodeRabbit to post actual review (not just "processing")
- Review any issues CodeRabbit identifies
- **VERIFY SUGGESTIONS LOCALLY** before implementing:
  - For CLI tool syntax: run `<tool> --help` to check actual interface
  - For API changes: check actual code/docs, not just CodeRabbit's claim
  - CodeRabbit may have outdated info about third-party tools
- **When fixing issues:** Look for the **Prompt for AI Agents** section in CodeRabbit's comments - it contains ready-to-use code snippets and instructions for implementing the suggested fix
- Fix blocking issues before merge

### Never Merge Until

- ALL CI checks show `pass`
- CodeRabbit review is complete (not "processing")
- No blocking issues from CodeRabbit

### Merge Command

```bash
gh pr merge <NUMBER> --squash
# Note: do NOT delete branches
```

## Checking PR Status

```bash
# Check CodeRabbit status
gh pr view <PR_NUMBER> --comments | grep -i coderabbit

# If still processing, WAIT and check again
```

## Lessons Learned

- **CodeRabbit Integration**: Never merge while CodeRabbit shows "processing" - must wait for actual review findings
- **Verify CodeRabbit Suggestions Locally**: NEVER blindly follow CodeRabbit suggestions. Always verify locally before implementing:
  - For CLI tool syntax issues, run `<tool> --help` to check actual command-line interface
  - For API suggestions, check the actual code or documentation
  - CodeRabbit may have outdated or incorrect information about third-party tools
