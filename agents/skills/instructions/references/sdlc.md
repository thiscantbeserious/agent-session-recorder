# SDLC Process

The Software Development Life Cycle using orchestrated roles.

## Roles

| Role | Phase | Responsibility |
|------|-------|----------------|
| Architect | Design | Creates plan, proposes options |
| Implementer | Code | Implements based on plan |
| Reviewer | Test | Validates against plan |
| Product Owner | Feedback | Final spec review, scope check |
| Maintainer | Deploy | Merges PR, handles releases |

## Flow

```
1. Design    → Architect creates .state/<branch>/plan.md
2. Code      → Implementer follows plan
3. Test      → Reviewer validates against plan
4. Feedback  → Product Owner reviews spec compliance
5. Deploy    → Maintainer merges after approvals
```

## Phases

### 1. Design (Architect)

- Understand requirements
- Propose 2-3 options with trade-offs
- Ask for user input
- Create plan in `.state/<branch-name>/plan.md`

### 2. Code (Implementer)

- Follow the plan
- TDD: write test first, then code
- Apply `coding-principles.md`
- Create PR when done

### 3. Test (Reviewer)

- Read the plan first
- Validate implementation matches plan
- Run tests: `cargo test && ./tests/e2e_test.sh`
- Check CodeRabbit review
- Report findings

### 4. Feedback (Product Owner)

- Validate against original requirements
- Check for scope creep
- Propose splitting side-work into new branches
- Final approval gate

### 5. Deploy (Maintainer)

- Merge PR after all approvals
- Handle release tagging if needed
- Clean up branches

## Cycle

```
Design → Code → Test → Feedback → Deploy
  ^                                 |
  +---------------------------------+
```

Each cycle delivers incremental value. Product Owner may trigger new cycles for split-out work.
