# Agile SDLC for AGR Development

This document describes the Agile Software Development Life Cycle used for AGR.

## Core Principles

Based on the Agile Manifesto:

- **Early customer involvement** - Get feedback early and often
- **Iterative development** - Work in short cycles (sprints)
- **Self-organizing teams** - Agents take ownership of tasks
- **Adaptation to change** - Respond to feedback, don't fight it

## Phases

### 1. Requirement Gathering

Collaborate to understand and prioritize needs.

**For AGR agents:**
```bash
# Check current state
cat .state/INDEX.md
cat .state/decisions.md
gh pr list --state merged    # What's been done
gh pr list                   # What's in progress
```

### 2. Design

Translate requirements into manageable tasks.

**For AGR agents:**
- Break down features into small, testable units
- Identify files to create/modify
- Consider edge cases
- Document significant decisions in `.state/decisions.md`

### 3. Coding (TDD Sprint)

Develop in short iterative cycles with frequent integration.

**For AGR agents - Red-Green-Refactor:**
1. Write failing test first (behavior-focused)
2. Run test - must fail
3. Write minimal code to pass
4. Run test - must pass
5. Refactor if needed
6. `cargo fmt` and `cargo clippy`
7. Commit

### 4. Testing / QA

Testing is integral to each iteration.

**For AGR agents:**
```bash
cargo test              # Unit tests (MUST pass)
./tests/e2e_test.sh     # E2E tests (MUST pass)
```

### 5. Deployment

Release increments frequently.

**For AGR agents:**
1. Create PR: `gh pr create`
2. Wait for CI (build, test, lint)
3. Wait for CodeRabbit review
4. Merge after verification: `gh pr merge --squash --delete-branch`

### 6. Feedback

Gather input to refine and improve.

**For AGR agents:**
- Document blockers encountered
- Note lessons learned in `.state/decisions.md`
- Update state files for next session

## Cycle

These phases work cyclically:

```
Requirement → Design → Code → Test → Deploy → Feedback
     ↑                                           │
     └───────────────────────────────────────────┘
```

Each cycle delivers incremental value while remaining responsive to change.

## Reference

- [Agile SDLC - GeeksforGeeks](https://www.geeksforgeeks.org/software-engineering/agile-sdlc-software-development-life-cycle/)
- [Agile Manifesto](https://agilemanifesto.org/)
