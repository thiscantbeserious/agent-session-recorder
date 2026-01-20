# Analyze Session Recording

Analyze the specified .cast file and add markers for interesting moments.

## Usage
/asr-analyze <path-to-file.cast>

## Process
1. Read the .cast file using the Read tool
2. Parse the JSON lines to extract terminal output (events with type "o")
3. Identify key moments:
   - Errors, exceptions, or failures
   - Important commands being executed
   - Decisions or turning points
   - Significant output or results
4. For each key moment, run:
   ```
   asr marker add <file.cast> <timestamp_seconds> "description"
   ```

## Understanding the Format

asciicast v3 format (newline-delimited JSON):
- First line: header with version, terminal size
- Subsequent lines: events as `[time, type, data]`
  - `time`: seconds since previous event
  - `type`: "o" (output), "i" (input), "m" (marker)
  - `data`: the actual text

To calculate absolute timestamps, sum up the times from the start.

## Example Analysis

If you find a build error at cumulative timestamp 45.2s:
```bash
asr marker add session.cast 45.2 "Build failed: missing dependency"
```

If you find a successful test completion at 120.5s:
```bash
asr marker add session.cast 120.5 "All tests passed"
```

## What to Look For

### Errors & Failures
- Stack traces
- "Error:", "ERROR", "error:"
- "Failed", "FAILED", "failure"
- Non-zero exit codes
- Compilation errors
- Test failures

### Decisions & Actions
- Git commits
- File creations/modifications
- Configuration changes
- Deployment commands

### Milestones
- "Build successful"
- "Tests passed"
- "Deployed to..."
- Task completions

### User Interactions
- Questions asked
- Confirmations given
- Choices made

## Output Format

After analysis, summarize the markers added:
```
Added X markers to session.cast:
  - 12.3s: Started build process
  - 45.2s: Build failed: missing dep
  - 67.8s: Fixed dependency issue
  - 89.1s: Build successful
```
