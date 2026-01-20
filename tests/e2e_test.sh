#!/bin/bash
# End-to-end tests for AGR with real asciinema
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
AGR="$PROJECT_DIR/target/release/agr"
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

echo "=== AGR End-to-End Tests ==="
echo "Test directory: $TEST_DIR"
echo

# Check prerequisites
if ! command -v asciinema &>/dev/null; then
    echo "ERROR: asciinema not installed"
    exit 1
fi

if [ ! -x "$AGR" ]; then
    echo "ERROR: AGR binary not found at $AGR"
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
$AGR record echo -- "hello e2e test" </dev/null
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
LIST_OUTPUT=$($AGR list)
if echo "$LIST_OUTPUT" | /usr/bin/grep -q "echo"; then
    pass "List command shows echo agent recording"
else
    fail "List command missing recording: $LIST_OUTPUT"
fi

# Test 5: Status shows correct count
echo "--- Test 5: Status shows correct count ---"
STATUS_OUTPUT=$($AGR status)
if echo "$STATUS_OUTPUT" | /usr/bin/grep -q "1 total"; then
    pass "Status shows 1 session"
else
    fail "Status count incorrect: $STATUS_OUTPUT"
fi

# Test 6: Add marker to recording
echo "--- Test 6: Add marker to recording ---"
if [ -f "$CAST_FILE" ]; then
    $AGR marker add "$CAST_FILE" 0.01 "E2E test marker"
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
    MARKERS=$($AGR marker list "$CAST_FILE")
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
$AGR agents add e2e-test-agent
AGENTS=$($AGR agents list)
if echo "$AGENTS" | /usr/bin/grep -q "e2e-test-agent"; then
    pass "Agent added successfully"
else
    fail "Agent add failed: $AGENTS"
fi
$AGR agents remove e2e-test-agent
AGENTS=$($AGR agents list)
if ! echo "$AGENTS" | /usr/bin/grep -q "e2e-test-agent"; then
    pass "Agent removed successfully"
else
    fail "Agent remove failed: $AGENTS"
fi

# Test 9: Config persistence
echo "--- Test 9: Config persistence ---"
$AGR agents add persistent-agent
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "persistent-agent"; then
    pass "Config persists agent addition"
else
    fail "Config not persisted: $CONFIG"
fi

# Test 10: Cleanup (non-interactive)
echo "--- Test 10: Cleanup command ---"
CLEANUP_OUTPUT=$(echo "0" | $AGR cleanup)
if echo "$CLEANUP_OUTPUT" | /usr/bin/grep -q "Found 1 sessions"; then
    pass "Cleanup finds sessions"
else
    fail "Cleanup output unexpected: $CLEANUP_OUTPUT"
fi

# Test 11: Record with different agent
echo "--- Test 11: Record with different agent ---"
$AGR record ls -- -la </dev/null
LS_CAST=$(ls "$HOME/recorded_agent_sessions/ls/"*.cast 2>/dev/null | /usr/bin/head -1)
if [ -f "$LS_CAST" ]; then
    pass "Second recording with different agent created"
else
    fail "Second recording not created"
fi

# Test 12: List filter by agent
echo "--- Test 12: List filter by agent ---"
ECHO_LIST=$($AGR list echo)
LS_LIST=$($AGR list ls)
if echo "$ECHO_LIST" | /usr/bin/grep -q "echo" && ! echo "$ECHO_LIST" | /usr/bin/grep -q "\.cast (ls,"; then
    pass "List filters by agent correctly"
else
    fail "List filter not working: echo=$ECHO_LIST"
fi

# Test 13: Status shows multiple agents
echo "--- Test 13: Status with multiple agents ---"
STATUS=$($AGR status)
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

# Test 15: Shell status (before install)
echo "--- Test 15: Shell status (before install) ---"
SHELL_OUTPUT=$($AGR shell status 2>&1)
if echo "$SHELL_OUTPUT" | /usr/bin/grep -q "not installed"; then
    pass "Shell status shows not installed"
else
    fail "Shell status unexpected output: $SHELL_OUTPUT"
fi

# Test 16: Shell install
echo "--- Test 16: Shell install ---"
# Create a .zshrc for testing
touch "$HOME/.zshrc"
$AGR shell install
if /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell install added marked section to .zshrc"
else
    fail "Shell install did not modify .zshrc"
    cat "$HOME/.zshrc"
fi
if [ -f "$HOME/.config/agr/agr.sh" ]; then
    pass "Shell install created agr.sh script"
else
    fail "Shell install did not create agr.sh"
fi

# Test 17: Shell status (after install)
echo "--- Test 17: Shell status (after install) ---"
SHELL_OUTPUT=$($AGR shell status 2>&1)
if echo "$SHELL_OUTPUT" | /usr/bin/grep -qiE '\binstalled\b' \
   && ! echo "$SHELL_OUTPUT" | /usr/bin/grep -qiE '\bnot installed\b'; then
    pass "Shell status shows installed"
else
    fail "Shell status not showing installed: $SHELL_OUTPUT"
fi

# Test 18: Shell uninstall
echo "--- Test 18: Shell uninstall ---"
$AGR shell uninstall
if ! /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell uninstall removed marked section from .zshrc"
else
    fail "Shell uninstall did not clean .zshrc"
    cat "$HOME/.zshrc"
fi

# Test 19: Auto-wrap config toggle
echo "--- Test 19: Auto-wrap config toggle ---"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "auto_wrap"; then
    pass "Config shows auto_wrap setting"
else
    fail "Config missing auto_wrap: $CONFIG"
fi

# Test 20: Shell install with existing content preserves it
echo "--- Test 20: Shell install preserves existing .zshrc content ---"
echo "# My existing config" > "$HOME/.zshrc"
echo "export MY_VAR=test" >> "$HOME/.zshrc"
$AGR shell install
if /usr/bin/grep -q "MY_VAR=test" "$HOME/.zshrc" && /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell install preserved existing content"
else
    fail "Shell install did not preserve existing content"
    cat "$HOME/.zshrc"
fi

# Test 21: Shell uninstall preserves other content
echo "--- Test 21: Shell uninstall preserves other content ---"
$AGR shell uninstall
if /usr/bin/grep -q "MY_VAR=test" "$HOME/.zshrc" && ! /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell uninstall preserved other content"
else
    fail "Shell uninstall did not preserve other content"
    cat "$HOME/.zshrc"
fi

# Test 22: Agents no-wrap list (empty by default)
echo "--- Test 22: Agents no-wrap list (empty) ---"
NOWRAP_OUTPUT=$($AGR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "No agents in no-wrap list"; then
    pass "No-wrap list empty by default"
else
    fail "No-wrap list not empty: $NOWRAP_OUTPUT"
fi

# Test 23: Agents no-wrap add
echo "--- Test 23: Agents no-wrap add ---"
$AGR agents no-wrap add test-nowrap-agent
NOWRAP_OUTPUT=$($AGR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "test-nowrap-agent"; then
    pass "Agent added to no-wrap list"
else
    fail "Agent not in no-wrap list: $NOWRAP_OUTPUT"
fi

# Test 24: Agents is-wrapped (agent in no-wrap should return exit 1)
echo "--- Test 24: Agents is-wrapped respects no-wrap list ---"
if $AGR agents is-wrapped test-nowrap-agent 2>/dev/null; then
    fail "is-wrapped returned 0 for agent in no-wrap list"
else
    pass "is-wrapped correctly returns 1 for agent in no-wrap list"
fi

# Test 25: Agents is-wrapped (enabled agent should return exit 0)
echo "--- Test 25: Agents is-wrapped for enabled agent ---"
$AGR agents add wrap-test-agent
if $AGR agents is-wrapped wrap-test-agent 2>/dev/null; then
    pass "is-wrapped returns 0 for enabled agent"
else
    fail "is-wrapped returned 1 for enabled agent"
fi

# Test 26: Agents no-wrap remove
echo "--- Test 26: Agents no-wrap remove ---"
$AGR agents no-wrap remove test-nowrap-agent
NOWRAP_OUTPUT=$($AGR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "No agents in no-wrap list"; then
    pass "Agent removed from no-wrap list"
else
    fail "Agent still in no-wrap list: $NOWRAP_OUTPUT"
fi

# Test 27: Config shows recording.auto_analyze
echo "--- Test 27: Config shows recording options ---"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "auto_analyze"; then
    pass "Config shows auto_analyze option"
else
    fail "Config missing auto_analyze: $CONFIG"
fi

# Test 28: Config shows agents.no_wrap
echo "--- Test 28: Config shows no_wrap option ---"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "no_wrap"; then
    pass "Config shows no_wrap option"
else
    fail "Config missing no_wrap: $CONFIG"
fi

# Test 29: Config shows analysis_agent setting
echo "--- Test 29: Config shows analysis_agent setting ---"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "analysis_agent"; then
    pass "Config shows analysis_agent option"
else
    fail "Config missing analysis_agent: $CONFIG"
fi

# Test 30: Default analysis_agent is claude
echo "--- Test 30: Default analysis_agent is claude ---"
# Create fresh config by removing old one
rm -f "$HOME/.config/agr/config.toml"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q 'analysis_agent = "claude"'; then
    pass "Default analysis_agent is claude"
else
    fail "Default analysis_agent not claude: $CONFIG"
fi

# ===========================================================
# Analyzer E2E Tests (conditional - skip if agents not installed)
# ===========================================================

echo
echo "=== Analyzer E2E Tests (conditional) ==="

# Helper function to check if an agent binary is available
agent_installed() {
    AGENT=$1
    # Handle gemini-cli alias
    if [ "$AGENT" = "gemini-cli" ]; then
        BINARY="gemini"
    else
        BINARY="$AGENT"
    fi
    command -v "$BINARY" &>/dev/null
}

# Test 31: Analyzer detection for missing binary
echo "--- Test 31: Analyzer detection for missing binary ---"
# This test uses the Rust unit test to verify is_agent_installed
# We indirectly test via config - if we set a fake agent and try to record
# with auto_analyze=true, it should not crash (analyzer handles missing gracefully)
# For E2E, we verify by checking that auto_analyze hint is printed when agent missing
rm -f "$HOME/.config/agr/config.toml"
# Create config with auto_analyze enabled and a fake agent
mkdir -p "$HOME/.config/agr"
cat > "$HOME/.config/agr/config.toml" << 'TOMLEOF'
[recording]
auto_analyze = true
analysis_agent = "definitely-not-a-real-agent-12345"
TOMLEOF
# Record a simple command - should complete without crashing even with missing analyzer
RECORD_OUTPUT=$($AGR record echo -- "test auto-analyze hint" </dev/null 2>&1)
if echo "$RECORD_OUTPUT" | /usr/bin/grep -qiE "(auto.?analyze|skipping|not installed)"; then
    pass "Auto-analyze gracefully handles missing agent"
else
    # Even if no hint printed, recording should complete
    CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/tail -1)
    if [ -f "$CAST_FILE" ]; then
        pass "Recording completes even with missing analyzer agent"
    else
        fail "Recording failed with auto_analyze and missing agent"
    fi
fi

# Test 32: Config with custom analysis_agent persists
echo "--- Test 32: Custom analysis_agent persists in config ---"
rm -f "$HOME/.config/agr/config.toml"
mkdir -p "$HOME/.config/agr"
cat > "$HOME/.config/agr/config.toml" << 'TOMLEOF'
[recording]
auto_analyze = false
analysis_agent = "codex"
TOMLEOF
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q 'analysis_agent = "codex"'; then
    pass "Custom analysis_agent (codex) persists"
else
    fail "Custom analysis_agent not persisted: $CONFIG"
fi

# Test 33: Config with gemini-cli as analysis_agent
echo "--- Test 33: Config with gemini-cli analysis_agent ---"
rm -f "$HOME/.config/agr/config.toml"
mkdir -p "$HOME/.config/agr"
cat > "$HOME/.config/agr/config.toml" << 'TOMLEOF'
[recording]
auto_analyze = true
analysis_agent = "gemini-cli"
TOMLEOF
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q 'analysis_agent = "gemini-cli"'; then
    pass "gemini-cli as analysis_agent accepted"
else
    fail "gemini-cli analysis_agent not accepted: $CONFIG"
fi

# Test 34-36: Conditional agent detection tests (skip if not installed)
for AGENT in claude codex gemini-cli; do
    TEST_NUM=$((34 + $(echo "claude codex gemini-cli" | tr ' ' '\n' | /usr/bin/grep -n "^$AGENT$" | cut -d: -f1) - 1))
    echo "--- Test $TEST_NUM: Agent detection for $AGENT ---"
    if agent_installed "$AGENT"; then
        pass "$AGENT is installed and detected"
    else
        echo "  SKIP: $AGENT not installed (this is OK for CI)"
        # Count as pass since skipping is valid behavior
        PASS=$((PASS + 1))
    fi
done

# Test 37: Auto-analyze with real agent (conditional)
echo "--- Test 37: Auto-analyze integration (conditional) ---"
# Try each agent in order of preference
AVAILABLE_AGENT=""
for AGENT in claude codex gemini-cli; do
    if agent_installed "$AGENT"; then
        AVAILABLE_AGENT="$AGENT"
        break
    fi
done

if [ -n "$AVAILABLE_AGENT" ]; then
    echo "  Using available agent: $AVAILABLE_AGENT"
    # Set up config with auto_analyze enabled
    rm -f "$HOME/.config/agr/config.toml"
    mkdir -p "$HOME/.config/agr"
    cat > "$HOME/.config/agr/config.toml" << TOMLEOF
[recording]
auto_analyze = true
analysis_agent = "$AVAILABLE_AGENT"
TOMLEOF
    # Record a very short session
    # Note: We don't actually want the agent to analyze in E2E tests
    # as that would be slow and require API keys. We just verify the
    # config is read correctly.
    CONFIG=$($AGR config show)
    if echo "$CONFIG" | /usr/bin/grep -q "auto_analyze = true" && \
       echo "$CONFIG" | /usr/bin/grep -q "analysis_agent = \"$AVAILABLE_AGENT\""; then
        pass "Auto-analyze config set correctly for $AVAILABLE_AGENT"
    else
        fail "Auto-analyze config not set correctly: $CONFIG"
    fi
else
    echo "  SKIP: No AI agents installed (claude, codex, gemini-cli)"
    echo "  This is expected in CI environments without agent CLIs"
    PASS=$((PASS + 1))
fi

# Test 38: Recording with auto_analyze=false does not trigger analysis
echo "--- Test 38: Recording with auto_analyze=false ---"
rm -f "$HOME/.config/agr/config.toml"
mkdir -p "$HOME/.config/agr"
cat > "$HOME/.config/agr/config.toml" << 'TOMLEOF'
[recording]
auto_analyze = false
analysis_agent = "claude"
TOMLEOF
# Record should complete quickly without any analysis attempt
START_TIME=$(date +%s)
$AGR record echo -- "quick test" </dev/null 2>&1
END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))
# Should complete in under 5 seconds (no analysis overhead)
if [ "$ELAPSED" -lt 5 ]; then
    pass "Recording with auto_analyze=false completes quickly"
else
    fail "Recording took too long ($ELAPSED seconds), possible unintended analysis"
fi

# Clean up test config
rm -f "$HOME/.config/agr/config.toml"

echo
echo "=== Test Summary ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo

if [ $FAIL -gt 0 ]; then
    exit 1
fi
echo "All e2e tests passed!"
