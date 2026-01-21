# Current Phase: [N] - [PHASE_NAME]

## Goal

[What this phase aims to achieve]

## Tasks

| Task | Status | PR |
|------|--------|-----|
| [Task 1] | PENDING | - |
| [Task 2] | PENDING | - |
| [Task 3] | PENDING | - |

## Quick Start

```bash
# Verify environment
cargo test && ./tests/e2e_test.sh

# Check state
cat .state/INDEX.md
gh pr list
```

## Git Workflow

```bash
# Create feature branch
git checkout main && git pull
git checkout -b feature/[phase]-[task-name]

# Work with TDD
cargo test                    # Must pass
./tests/e2e_test.sh          # Must pass
cargo fmt && cargo clippy    # Must pass

# Commit and push
git add -A
git commit -m "feat(scope): description"
git push -u origin feature/[phase]-[task-name]

# Create PR
gh pr create --title "feat(scope): description"
```

## Definition of Done

- [ ] All tasks complete
- [ ] All tests pass (`cargo test`)
- [ ] E2E tests pass (`./tests/e2e_test.sh`)
- [ ] PR reviewed (CodeRabbit + verification)
- [ ] PR merged
- [ ] `.state/INDEX.md` updated

## Notes

[Any relevant notes for this phase]
