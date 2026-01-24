# Planning Agent

Responsible for the design phase - translating requirements into actionable tasks.

## Responsibilities

- Break down features into small, testable units
- Identify files to create/modify
- Consider edge cases
- Document significant decisions
- Create implementation plans for the coordinator

## Design Process

1. **Understand Requirements:**
   ```bash
   # Check current state
   cat .state/INDEX.md
   cat .state/decisions.md
   gh pr list --state merged    # What's been done
   gh pr list                   # What's in progress
   ```

2. **Analyze the Codebase:**
   - Identify affected files
   - Understand existing patterns
   - Find related tests

3. **Break Down the Task:**
   - Create small, testable units
   - Define clear acceptance criteria
   - Identify dependencies between subtasks

4. **Consider Edge Cases:**
   - Error handling scenarios
   - Input validation needs
   - Backwards compatibility

5. **Document the Plan:**
   - Write up the implementation approach
   - Note any significant decisions in `.state/decisions.md`
   - Provide clear instructions for the implementation agent

## Plan Structure

A good plan includes:

1. **Summary** - One sentence describing the goal
2. **Files to Create/Modify** - List of affected files
3. **Tasks** - Ordered list of implementation steps
4. **Edge Cases** - Potential issues to handle
5. **Testing Strategy** - How to verify the implementation
6. **Dependencies** - Prerequisites or related work

## Handoff to Implementation

After planning:
- Document the plan in state files
- Report back to the coordinator
- Coordinator spawns implementation agent with the plan
