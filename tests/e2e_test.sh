#!/bin/bash
# End-to-end tests for ASR with real asciinema
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ASR="$PROJECT_DIR/target/release/asr"
TEST_DIR=$(mktemp -d)
ORIGINAL_HOME="$HOME"

# Use test directory as home to isolate config
export HOME="$TEST_DIR"
mkdir -p "$HOME/recorded_agent_sessions"

cleanup() {
    export HOME="$ORIGINAL_HOME"
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

echo "=== ASR End-to-End Tests ==="
echo "Test directory: $TEST_DIR"
echo

# Check prerequisites
if ! command -v asciinema &>/dev/null; then
    echo "ERROR: asciinema not installed"
    exit 1
fi

if [ ! -x "$ASR" ]; then
    echo "ERROR: ASR binary not found at $ASR"
    echo "Run 'cargo build --release' first"
    exit 1
fi

PASS=0
FAIL=0

pass() {
    echo "✅ PASS: $1"
    PASS=$((PASS + 1))
}

fail() {
    echo "❌ FAIL: $1"
    FAIL=$((FAIL + 1))
}

# Test 1: Record a simple command
echo "--- Test 1: Record simple command ---"
$ASR record echo -- "hello e2e test" </dev/null
CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/head -1)
if [ -f "$CAST_FILE" ]; then
    pass "Recording created file"
else
    fail "Recording did not create file"
fi

# Test 2: Verify cast file is valid asciicast v3
echo "--- Test 2: Verify asciicast v3 format ---"
if [ -f "$CAST_FILE" ]; then
    HEADER=$(/usr/bin/head -1 "$CAST_FILE")
    if echo "$HEADER" | /usr/bin/grep -q '"version":3'; then
        pass "Cast file has version 3 header"
    else
        fail "Cast file missing version 3 header: $HEADER"
    fi
else
    fail "No cast file to verify"
fi

# Test 3: Verify output was captured
echo "--- Test 3: Verify output captured ---"
if [ -f "$CAST_FILE" ]; then
    if /usr/bin/grep -q "hello e2e test" "$CAST_FILE"; then
        pass "Output 'hello e2e test' captured in recording"
    else
        fail "Output not captured in recording"
        cat "$CAST_FILE"
    fi
else
    fail "No cast file to verify"
fi

# Test 4: List shows the recording
echo "--- Test 4: List command shows recording ---"
LIST_OUTPUT=$($ASR list)
if echo "$LIST_OUTPUT" | /usr/bin/grep -q "echo"; then
    pass "List command shows echo agent recording"
else
    fail "List command missing recording: $LIST_OUTPUT"
fi

# Test 5: Status shows correct count
echo "--- Test 5: Status shows correct count ---"
STATUS_OUTPUT=$($ASR status)
if echo "$STATUS_OUTPUT" | /usr/bin/grep -q "1 total"; then
    pass "Status shows 1 session"
else
    fail "Status count incorrect: $STATUS_OUTPUT"
fi

# Test 6: Add marker to recording
echo "--- Test 6: Add marker to recording ---"
if [ -f "$CAST_FILE" ]; then
    $ASR marker add "$CAST_FILE" 0.01 "E2E test marker"
    if /usr/bin/grep -q "E2E test marker" "$CAST_FILE"; then
        pass "Marker added to cast file"
    else
        fail "Marker not found in cast file"
    fi
else
    fail "No cast file for marker test"
fi

# Test 7: List markers shows the marker
echo "--- Test 7: List markers ---"
if [ -f "$CAST_FILE" ]; then
    MARKERS=$($ASR marker list "$CAST_FILE")
    if echo "$MARKERS" | /usr/bin/grep -q "E2E test marker"; then
        pass "Marker list shows added marker"
    else
        fail "Marker not in list: $MARKERS"
    fi
else
    fail "No cast file for marker list test"
fi

# Test 8: Agents add/remove
echo "--- Test 8: Agents add/remove ---"
$ASR agents add e2e-test-agent
AGENTS=$($ASR agents list)
if echo "$AGENTS" | /usr/bin/grep -q "e2e-test-agent"; then
    pass "Agent added successfully"
else
    fail "Agent add failed: $AGENTS"
fi
$ASR agents remove e2e-test-agent
AGENTS=$($ASR agents list)
if ! echo "$AGENTS" | /usr/bin/grep -q "e2e-test-agent"; then
    pass "Agent removed successfully"
else
    fail "Agent remove failed: $AGENTS"
fi

# Test 9: Config persistence
echo "--- Test 9: Config persistence ---"
$ASR agents add persistent-agent
CONFIG=$($ASR config show)
if echo "$CONFIG" | /usr/bin/grep -q "persistent-agent"; then
    pass "Config persists agent addition"
else
    fail "Config not persisted: $CONFIG"
fi

# Test 10: Cleanup (non-interactive)
echo "--- Test 10: Cleanup command ---"
CLEANUP_OUTPUT=$(echo "0" | $ASR cleanup)
if echo "$CLEANUP_OUTPUT" | /usr/bin/grep -q "Found 1 sessions"; then
    pass "Cleanup finds sessions"
else
    fail "Cleanup output unexpected: $CLEANUP_OUTPUT"
fi

# Test 11: Record with different agent
echo "--- Test 11: Record with different agent ---"
$ASR record ls -- -la </dev/null
LS_CAST=$(ls "$HOME/recorded_agent_sessions/ls/"*.cast 2>/dev/null | /usr/bin/head -1)
if [ -f "$LS_CAST" ]; then
    pass "Second recording with different agent created"
else
    fail "Second recording not created"
fi

# Test 12: List filter by agent
echo "--- Test 12: List filter by agent ---"
ECHO_LIST=$($ASR list echo)
LS_LIST=$($ASR list ls)
if echo "$ECHO_LIST" | /usr/bin/grep -q "echo" && ! echo "$ECHO_LIST" | /usr/bin/grep -q "\.cast (ls,"; then
    pass "List filters by agent correctly"
else
    fail "List filter not working: echo=$ECHO_LIST"
fi

# Test 13: Status shows multiple agents
echo "--- Test 13: Status with multiple agents ---"
STATUS=$($ASR status)
if echo "$STATUS" | /usr/bin/grep -q "2 total" && echo "$STATUS" | /usr/bin/grep -q "echo:" && echo "$STATUS" | /usr/bin/grep -q "ls:"; then
    pass "Status shows both agents"
else
    fail "Status not showing both agents: $STATUS"
fi

# Test 14: Cast file has proper events structure
echo "--- Test 14: Cast file event structure ---"
if [ -f "$CAST_FILE" ]; then
    # Check for output event format [time, "o", "data"]
    if /usr/bin/grep -E '^\[.*"o".*\]' "$CAST_FILE" >/dev/null; then
        pass "Cast file has proper output event structure"
    else
        fail "Cast file missing proper event structure"
    fi
else
    fail "No cast file to check structure"
fi

# Test 15: Skills list (before install)
echo "--- Test 15: Skills list (before install) ---"
SKILLS_OUTPUT=$($ASR skills list 2>&1)
if echo "$SKILLS_OUTPUT" | /usr/bin/grep -q "asr-analyze"; then
    pass "Skills list shows embedded asr-analyze skill"
else
    fail "Skills list missing asr-analyze: $SKILLS_OUTPUT"
fi

# Test 16: Skills install
echo "--- Test 16: Skills install ---"
$ASR skills install
if [ -f "$HOME/.claude/commands/asr-analyze.md" ]; then
    pass "Skills install created asr-analyze.md in .claude/commands"
else
    fail "Skills install did not create asr-analyze.md"
fi
if [ -f "$HOME/.claude/commands/asr-review.md" ]; then
    pass "Skills install created asr-review.md in .claude/commands"
else
    fail "Skills install did not create asr-review.md"
fi

# Test 17: Skills list shows installed location
echo "--- Test 17: Skills list shows installed status ---"
SKILLS_OUTPUT=$($ASR skills list 2>&1)
if echo "$SKILLS_OUTPUT" | /usr/bin/grep -qiE '\binstalled\b' \
   && ! echo "$SKILLS_OUTPUT" | /usr/bin/grep -qiE '\bnot installed\b'; then
    pass "Skills list shows installed status"
else
    fail "Skills list not showing installed status: $SKILLS_OUTPUT"
fi

# Test 18: Skills uninstall
echo "--- Test 18: Skills uninstall ---"
$ASR skills uninstall
if [ ! -f "$HOME/.claude/commands/asr-analyze.md" ]; then
    pass "Skills uninstall removed asr-analyze.md"
else
    fail "Skills uninstall did not remove asr-analyze.md"
fi

# Test 19: Shell status (before install)
echo "--- Test 19: Shell status (before install) ---"
SHELL_OUTPUT=$($ASR shell status 2>&1)
if echo "$SHELL_OUTPUT" | /usr/bin/grep -q "not installed"; then
    pass "Shell status shows not installed"
else
    fail "Shell status unexpected output: $SHELL_OUTPUT"
fi

# Test 20: Shell install
echo "--- Test 20: Shell install ---"
# Create a .zshrc for testing
touch "$HOME/.zshrc"
$ASR shell install
if /usr/bin/grep -q "ASR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell install added marked section to .zshrc"
else
    fail "Shell install did not modify .zshrc"
    cat "$HOME/.zshrc"
fi
if [ -f "$HOME/.config/asr/asr.sh" ]; then
    pass "Shell install created asr.sh script"
else
    fail "Shell install did not create asr.sh"
fi

# Test 21: Shell status (after install)
echo "--- Test 21: Shell status (after install) ---"
SHELL_OUTPUT=$($ASR shell status 2>&1)
if echo "$SHELL_OUTPUT" | /usr/bin/grep -qiE '\binstalled\b' \
   && ! echo "$SHELL_OUTPUT" | /usr/bin/grep -qiE '\bnot installed\b'; then
    pass "Shell status shows installed"
else
    fail "Shell status not showing installed: $SHELL_OUTPUT"
fi

# Test 22: Shell uninstall
echo "--- Test 22: Shell uninstall ---"
$ASR shell uninstall
if ! /usr/bin/grep -q "ASR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell uninstall removed marked section from .zshrc"
else
    fail "Shell uninstall did not clean .zshrc"
    cat "$HOME/.zshrc"
fi

# Test 23: Auto-wrap config toggle
echo "--- Test 23: Auto-wrap config toggle ---"
CONFIG=$($ASR config show)
if echo "$CONFIG" | /usr/bin/grep -q "auto_wrap"; then
    pass "Config shows auto_wrap setting"
else
    fail "Config missing auto_wrap: $CONFIG"
fi

# Test 24: Shell install with existing content preserves it
echo "--- Test 24: Shell install preserves existing .zshrc content ---"
echo "# My existing config" > "$HOME/.zshrc"
echo "export MY_VAR=test" >> "$HOME/.zshrc"
$ASR shell install
if /usr/bin/grep -q "MY_VAR=test" "$HOME/.zshrc" && /usr/bin/grep -q "ASR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell install preserved existing content"
else
    fail "Shell install did not preserve existing content"
    cat "$HOME/.zshrc"
fi

# Test 25: Shell uninstall preserves other content
echo "--- Test 25: Shell uninstall preserves other content ---"
$ASR shell uninstall
if /usr/bin/grep -q "MY_VAR=test" "$HOME/.zshrc" && ! /usr/bin/grep -q "ASR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell uninstall preserved other content"
else
    fail "Shell uninstall did not preserve other content"
    cat "$HOME/.zshrc"
fi

# Test 26: Agents no-wrap list (empty by default)
echo "--- Test 26: Agents no-wrap list (empty) ---"
NOWRAP_OUTPUT=$($ASR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "No agents in no-wrap list"; then
    pass "No-wrap list empty by default"
else
    fail "No-wrap list not empty: $NOWRAP_OUTPUT"
fi

# Test 27: Agents no-wrap add
echo "--- Test 27: Agents no-wrap add ---"
$ASR agents no-wrap add test-nowrap-agent
NOWRAP_OUTPUT=$($ASR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "test-nowrap-agent"; then
    pass "Agent added to no-wrap list"
else
    fail "Agent not in no-wrap list: $NOWRAP_OUTPUT"
fi

# Test 28: Agents is-wrapped (agent in no-wrap should return exit 1)
echo "--- Test 28: Agents is-wrapped respects no-wrap list ---"
if $ASR agents is-wrapped test-nowrap-agent 2>/dev/null; then
    fail "is-wrapped returned 0 for agent in no-wrap list"
else
    pass "is-wrapped correctly returns 1 for agent in no-wrap list"
fi

# Test 29: Agents is-wrapped (enabled agent should return exit 0)
echo "--- Test 29: Agents is-wrapped for enabled agent ---"
$ASR agents add wrap-test-agent
if $ASR agents is-wrapped wrap-test-agent 2>/dev/null; then
    pass "is-wrapped returns 0 for enabled agent"
else
    fail "is-wrapped returned 1 for enabled agent"
fi

# Test 30: Agents no-wrap remove
echo "--- Test 30: Agents no-wrap remove ---"
$ASR agents no-wrap remove test-nowrap-agent
NOWRAP_OUTPUT=$($ASR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "No agents in no-wrap list"; then
    pass "Agent removed from no-wrap list"
else
    fail "Agent still in no-wrap list: $NOWRAP_OUTPUT"
fi

# Test 31: Config shows recording.auto_analyze
echo "--- Test 31: Config shows recording options ---"
CONFIG=$($ASR config show)
if echo "$CONFIG" | /usr/bin/grep -q "auto_analyze"; then
    pass "Config shows auto_analyze option"
else
    fail "Config missing auto_analyze: $CONFIG"
fi

# Test 32: Config shows agents.no_wrap
echo "--- Test 32: Config shows no_wrap option ---"
CONFIG=$($ASR config show)
if echo "$CONFIG" | /usr/bin/grep -q "no_wrap"; then
    pass "Config shows no_wrap option"
else
    fail "Config missing no_wrap: $CONFIG"
fi

echo
echo "=== Test Summary ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo

if [ $FAIL -gt 0 ]; then
    exit 1
fi
echo "All e2e tests passed!"
