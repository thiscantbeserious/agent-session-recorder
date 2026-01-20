# Code Review

Review the specified PR for quality and correctness.

## Usage
/agr-review <pr-number>

## Process
1. Fetch PR information: `gh pr view <pr-number>`
2. Fetch PR diff: `gh pr diff <pr-number>`
3. Read changed files for context
4. Review against checklist
5. Post review with approval or change requests

## Review Checklist

### Tests
- [ ] Tests exist for new functionality
- [ ] Tests follow TDD (behavior-focused, not implementation-focused)
- [ ] All tests pass (`cargo test`)
- [ ] Coverage maintained ≥80%

### Code Quality
- [ ] Code compiles without warnings (`cargo build`)
- [ ] No obvious bugs or logic errors
- [ ] Error handling is present and appropriate
- [ ] Consistent style with existing code
- [ ] No security vulnerabilities

### Documentation
- [ ] Public functions have doc comments
- [ ] Complex logic has explanatory comments
- [ ] AGENTS.md updated if needed

### Architecture
- [ ] Changes are minimal and focused
- [ ] No unnecessary complexity
- [ ] Follows existing patterns

## Review Commands

Approve the PR:
```bash
gh pr review <pr-number> --approve --body "LGTM! ..."
```

Request changes:
```bash
gh pr review <pr-number> --request-changes --body "Please address: ..."
```

Add a comment:
```bash
gh pr review <pr-number> --comment --body "..."
```

## Example Review

```bash
# Get PR info
gh pr view 42

# Get the diff
gh pr diff 42

# After review, approve
gh pr review 42 --approve --body "$(cat <<'EOF'
## Review Summary

- Tests are comprehensive and behavior-focused
- Code is clean and follows existing patterns
- Error handling is appropriate

LGTM!
EOF
)"
```

## What to Flag

### Must Fix
- Failing tests
- Security vulnerabilities
- Missing error handling for edge cases
- Breaking changes without migration

### Should Fix
- Missing tests for new code paths
- Inconsistent naming/style
- Overly complex solutions
- Missing documentation

### Consider
- Performance improvements
- Code organization suggestions
- Alternative approaches
