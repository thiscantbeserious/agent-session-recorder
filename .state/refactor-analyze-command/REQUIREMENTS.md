# Requirements: Refactor Analyze Command

**Branch:** refactor-analyze-command
**Date:** 2026-02-04
**Sign-off:** Approved by user (2026-02-04)

---

## Problem Statement

### Target Audience

Engineers reviewing AI agent sessions - including reviewing their own work. The analysis helps engineers understand what happened during a session by marking key engineering workflow moments.

### Current Issues

The current `analyze` command is a rudimentary prototype that has two critical issues preventing production use:

### Problem 1: File Size

Cast files regularly grow to 60-75MB and can easily exceed 100MB due to ANSI escape sequences (terminal control codes for cursor positioning, colors, styling). These codes are:
- Unnecessary for LLM analysis (the LLM needs semantic content, not rendering instructions)
- Causing analysis to take extremely long or timeout
- Potentially exceeding LLM context windows

Example from a real 75MB cast file:
```
[0.164,"o","\u001b[?2026h...\u001b[38;5;174m...\u001b[48;5;174m\u001b[38;5;16m..."]
```

### Problem 2: Architecture & Permission Flags

The current implementation:
- Spawns agent CLIs (claude, codex, gemini) but **lacks required permission flags**
- Different agents have different permission requirements that aren't being handled
- Expects spawned agents to execute `agr marker add` commands (requires shell permissions)
- Has no abstraction layers for different agent types
- Cannot grow or scale as requirements evolve

**The core issue:** Agent binaries ARE being called, but they need specific permission flags to work properly (e.g., for reading files, returning structured output). Each agent CLI may have different flag requirements. This needs Architect research.

---

## Requirements

### R1: In-Memory ANSI Stripping for Analysis

The analyzer MUST strip ANSI escape sequences from cast file content before sending to the LLM.

**Constraints:**
- Stripping MUST be in-memory/temporary only
- Original cast files MUST NOT be modified
- Playback quality MUST remain unaffected
- The stripped representation should preserve:
  - Timestamps (for marker positioning)
  - Readable text content
  - Session structure (commands, outputs, timing)

**Rationale:** ANSI codes are rendering instructions that add no semantic value for analysis but dramatically increase file size and potentially confuse LLMs.

### R2: Proper Abstraction Layers (Architect Research Required)

The analyzer MUST be refactored with proper abstraction to support growth and scaling.

**Goals:**
- Clean separation of concerns
- Each component independently testable
- New agents can be added without modifying core logic
- Architecture can evolve as requirements change

**Constraints:**
- Must work with Claude, Codex, and Gemini CLIs
- Agents should not require elevated permissions if avoidable
- Current monolithic design cannot grow - needs restructuring

**Architect to research and propose:**
- How to structure the abstraction layers
- How to minimize or eliminate agent permission requirements
- Present options with trade-offs before implementation

**Rationale:** The current monolithic `spawn_agent()` function cannot grow. Architect should propose a scalable architecture.

**Ideas to consider (not prescriptive):**
- Input transformation layer for converting cast data
- Agent interface trait for different backends
- Output parsing for structured responses
- Permission-free approaches (e.g., sending data to agents vs agents reading files)
- Permission-flag approaches if file access is needed

### R3: Structured Analysis Results

The analyzer MUST return structured results that can be reliably parsed and applied as markers.

**Goals:**
- Agents return structured data (e.g., JSON) that agr can parse
- Marker writing is handled by agr, not by agents executing commands
- Minimize or eliminate agent permission requirements
- Clear separation between analysis (agent) and file I/O (agr)

**Required output schema (minimum fields):**
- Timestamp - when the moment occurred
- Label/description - what happened
- Category - type of engineering moment

**Engineering-focused marker categories:**
- **Planning** - Planning phases, task breakdown, approach decisions
- **Design/ADR** - Architecture decision moments, design choices
- **Implementation** - Implementation attempts, coding phases
- **Success** - What worked well, successful outcomes
- **Failure** - What didn't work, failed attempts, issues encountered

**Rationale:** The current approach of agents executing shell commands is fragile and creates permission issues. Structured output enables agr to own marker writing.

**Architect to determine:** The specific data flow, communication method, and how to achieve these goals.

### R4: Playback Compatibility

All changes MUST maintain full compatibility with:
- The built-in AGR player (`agr play`)
- Standard asciinema players
- The asciicast v3 file format specification

**Verification:**
- Cast files with markers must play back correctly
- Markers must be navigable in the player (jump to marker)
- No corruption of timing, events, or header data

### R5: Parallel Analysis (Core Goal)

Parallel analysis is a **core target** for this refactor, not a "nice to have." Large files should be analyzed quickly by splitting work across multiple agents.

**Requirements:**
- Split large files into segments, analyze in parallel, merge results
- Number of parallel agents MUST be configurable
- Include retry logic for failed segments
- If parallel analysis fails repeatedly, fall back to sequential analysis
- Failures must be transparent to the user (clear error messages)

**Resource awareness:**
- Usage is subscription-based (Claude, Codex, Gemini subscriptions), not per-token API billing
- Still be resource-sensitive - don't waste compute unnecessarily
- Track token/resource usage to inform smart decisions (e.g., when to parallelize vs go sequential)

**Rationale:** Quick results are critical. A 100MB+ file should not take 10+ minutes. Parallel processing dramatically reduces wall-clock time.

### R6: Token Tracking & Smart Decisions

The analyzer MUST track resource usage to make informed decisions.

**Requirements:**
- Track tokens/resources used during analysis
- Use tracking data to inform decisions (e.g., file size thresholds for parallelization)
- Provide visibility into resource usage (for user awareness, not billing)

**Scope:** Basic tracking for current decision-making. Self-learning/adaptive features are out of scope (see Future Considerations).

**Rationale:** Without usage data, we can't optimize. Tracking enables smart choices about when to parallelize, how many agents to use, etc.

### R7: Pattern-Driven Architecture (Architect Research Required)

The Architect MUST apply established architectural patterns. The architecture should be pattern-driven, not ad-hoc.

**Requirements:**
- Use appropriate design patterns for each component
- Document and justify pattern choices in the ADR
- Patterns should be well-known where possible

**Rationale:** Ad-hoc abstractions lead to inconsistent code. Pattern-driven design provides proven solutions and clear extension points.

### R8: Error Handling & Smart Retry (Architect Research Required)

Analysis must provide clear feedback and handle failures intelligently.

**Requirements:**
- Clear error messages when analysis fails
- Transparent reporting of what went wrong and where
- Retry logic should be informed by token usage data (retry first based on token tracking)
- Simple, understandable output (not technical stack traces)

**Architect research needed:**
- How should retry logic use token tracking to make smart decisions?
- What thresholds or heuristics determine when to retry vs fail?
- How does this integrate with parallel analysis fallback (R5)?

**Rationale:** Users need to understand what happened. Smart retry based on resource usage avoids wasting resources on doomed retries.

### R9: Existing Marker Handling (Idempotency)

Re-running analysis on a file with existing markers should be safe.

**Requirements:**
- Warn the user if the file already has markers
- Do NOT prevent re-running analysis (just warn)
- No complex "smart logic" to deduplicate or merge with existing markers

**Rationale:** Keep it simple. Users may want to re-analyze. Warn them, but don't block them.

---

## Out of Scope

- Permanent ANSI stripping (affects playback quality)
- Changes to the recording mechanism
- Changes to the asciicast file format
- Adding new agent integrations (just abstracting existing ones)
- UI changes to the TUI or player
- Self-learning/adaptive token service (see Future Considerations)

---

## Future Considerations

Ideas for potential future evolution (out of scope for this refactor, but Architect may reference in ADR):

**Self-Learning Token Service**
- Adaptive token tracking with embedded limits per agent
- Could learn usage patterns over time and auto-adjust thresholds
- Per-agent intelligence (Claude vs Codex vs Gemini may have different optimal settings)
- Currently overkill - start with basic tracking (R6) and evolve if needed

---

## Acceptance Criteria

### AC1: Automatic Marker Addition
- [ ] Running `agr analyze <file>` successfully adds markers to the cast file
- [ ] Markers are added without manual intervention
- [ ] Works for files exceeding 100MB

### AC2: File Integrity
- [ ] Files are not corrupted by analysis
- [ ] Files play back correctly after analysis
- [ ] Existing markers are preserved (new markers are added alongside)

### AC3: Marker Quality
- [ ] Markers have meaningful descriptions
- [ ] Markers are positioned at correct timestamps
- [ ] Markers identify engineering workflow moments:
  - Planning phases
  - Design/ADR decisions
  - Implementation attempts
  - Success moments (what worked)
  - Failure moments (what didn't work)

### AC4: Performance
- [ ] Analysis completes quickly (large files don't take 10+ minutes)
- [ ] Parallel analysis works for large files
- [ ] Configurable parallelism (user can adjust)
- [ ] Graceful fallback to sequential if parallel fails

### AC5: User Experience
- [ ] Clear error messages when things fail
- [ ] Warning displayed if file already has markers
- [ ] Smart retry logic informed by token usage
- [ ] Resource usage is tracked and visible

### AC6: Architecture
- [ ] Clean separation of concerns
- [ ] New agents can be added without major changes
- [ ] Pattern choices documented in ADR
- [ ] Permission requirements minimized or justified
- [ ] Design approach documented with rationale and trade-offs

---

## Technical Context

### Existing Code Structure

| File | Purpose |
|------|---------|
| `src/commands/analyze.rs` | CLI handler - resolves path, checks agent, invokes Analyzer |
| `src/analyzer.rs` | Core logic - builds prompt, spawns agent subprocess |
| `src/asciicast/marker.rs` | Marker operations - add/list markers in cast files |
| `src/asciicast/types.rs` | Data types - Header, Event, EventType, AsciicastFile |
| `src/asciicast/reader.rs` | File parsing - parse cast files from various sources |
| `src/asciicast/writer.rs` | File writing - serialize cast files |

### Marker Format (asciicast v3)

Markers are events with type "m":
```json
[0.1, "m", "ERROR: Build failed - missing dependency"]
```

Markers are inserted at the correct position in the event stream and timing is adjusted to maintain playback accuracy.

### Current Agent Invocation (Issues)

The existing implementation:
1. Calls agent CLIs (claude, codex, gemini) with basic flags
2. Sends prompt asking agent to read cast file and execute shell commands
3. Uses generic categories ("errors, milestones, key decisions")

**Problems to address:**
- Agents are asked to read files and execute commands (creates permission issues)
- Categories too generic - need engineering workflow focus
- No clear separation between data processing and file I/O

**Architectural goals:**
- Minimize or eliminate agent permission requirements
- Agents return structured data that can be parsed
- agr owns marker writing (not delegated to agents via shell commands)
- Clean separation of concerns

*Architect determines how to achieve these goals.*

---

## Notes

- This should be done "properly" - well-architected with a solid foundation
- The existing prototype provides insight but should not constrain the new design
- Quick results are a priority - users should not wait 10+ minutes for analysis
- Parallel analysis is a core goal, not a "maybe later" feature
- Usage is subscription-based (not per-token API), but still be resource-conscious
- Requirements express WHAT we need - the Architect determines HOW
- R2, R5, R7, R8 require Architect research - these are goals, not implementation specifications

**Architect freedom:**
- Architect should research approaches and present options with trade-offs
- No predefined solutions - Architect proposes based on research
- Preference is for minimizing agent permissions, but Architect determines feasibility
