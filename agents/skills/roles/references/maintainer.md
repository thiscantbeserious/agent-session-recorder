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

3. Merge after approval:
   ```bash
   gh pr merge <PR_NUMBER> --squash
   ```

4. Update ADR Status to Accepted in `.state/<branch-name>/ADR.md`

## Key Rules

- Never merge without reviewer approval
- Never merge while CI is failing
- Never merge while CodeRabbit shows "processing"
- Use squash merges to keep history clean
