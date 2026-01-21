# Implementation Agent

Spawned per-task to implement features on feature branches.

## Responsibilities

- Follow TDD: Red-Green-Refactor cycle
- Run `cargo test` and `./tests/e2e_test.sh`
- Create PR with clear description
- Create PR with progress
- Update `.state/INDEX.md` if needed

## Workflow

1. Claim task via lock file
2. Create feature branch
3. Implement with TDD
4. Run all tests
5. Create PR
6. Report completion

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
```

## TDD Cycle

1. Write failing test first (behavior-focused)
2. Run test - must fail
3. Write minimal code to pass
4. Run test - must pass
5. Refactor if needed
6. `cargo fmt` and `cargo clippy`
7. Commit

## Verification Before PR

```bash
cargo fmt          # Format code
cargo clippy       # Lint for common issues
cargo test         # Run all tests
cargo build --release  # Build release binary
./tests/e2e_test.sh    # E2E tests
```

## Task Claiming

Before working on a task:
```bash
# Check if task is claimed
if [ -f .state/locks/task-name.lock ]; then
  echo "Task claimed, pick another"
  exit 0
fi
# Claim it
echo "$(date +%s)" > .state/locks/task-name.lock
```

After completing:
```bash
rm .state/locks/task-name.lock
```
