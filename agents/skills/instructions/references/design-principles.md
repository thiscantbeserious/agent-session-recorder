# Design Principles

How to evaluate approaches and choose solutions that scale.

This guide covers:
- **SOLID** - Foundational principles for maintainable systems
- **Clean Code** - Pragmatic philosophy for sustainable code
- **Problem Decomposition** - Breaking work into focused pieces
- **Trade-off Evaluation** - Choosing between approaches

**Goal:** Propose the best plan - one that solves the problem today and scales for tomorrow.

## SOLID Principles

Adapted for Rust - not all apply directly.

**Single Responsibility**
One module, one reason to change. If describing what a module does requires "and", split it.

**Open/Closed**
Extend behavior through composition and new types, not modifying existing code. Add new match arms, don't change existing ones.

**Dependency Inversion**
Depend on abstractions at system boundaries (traits for external services, configs for behavior). Internal code can be concrete.

## Clean Code Philosophy

**KISS** - Simplest solution that works. Complexity must justify itself with concrete benefits.

**YAGNI** - Don't build for hypothetical futures. Solve today's problem.

**DRY** - Avoid repetition, but don't over-abstract. Duplication is better than the wrong abstraction.

## Problem Decomposition

**One thing at a time**
Each PR/branch addresses one concern. Mixed concerns get split.

**Domain grouping**
Related changes stay together. A "user auth" change doesn't include "logging refactor".

**Small iterations**
Prefer many small cycles over one big-bang change. Each iteration is independently shippable.

## Trade-off Evaluation

When comparing approaches:

| Criterion | Question |
|-----------|----------|
| Simplicity | Which is easier to understand? |
| Maintainability | Which will be easier to modify later? |
| Testability | Which is easier to test in isolation? |
| Consistency | Which fits existing patterns? |

**Priority:** Simplicity > Maintainability > Testability > Consistency

Performance rarely matters at design time. Optimize later when you have data.

## Checklist

Before finalizing a design:

- [ ] Single responsibility per module?
- [ ] Simplest approach that solves the problem?
- [ ] One concern per PR/branch?
- [ ] Small enough to review in one sitting?
- [ ] Fits SDLC (testable stages)?
