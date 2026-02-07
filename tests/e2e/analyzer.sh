#!/bin/bash
# Auto-analyze configuration tests for AGR
# Tests: analysis agent config, auto_analyze toggle, conditional agent detection

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    check_prerequisites || exit 1
    section "AGR Analyzer Configuration Tests"
    echo "Test directory: $TEST_DIR"
fi

# Test: Config shows recording.auto_analyze
test_header "Config shows recording options"
reset_config
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "auto_analyze"; then
    pass "Config shows auto_analyze option"
else
    fail "Config missing auto_analyze: $CONFIG"
fi

# Test: Config shows agents.no_wrap
test_header "Config shows no_wrap option"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "no_wrap"; then
    pass "Config shows no_wrap option"
else
    fail "Config missing no_wrap: $CONFIG"
fi

# Test: Config shows analysis agent setting
test_header "Config shows analysis agent setting"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "agent"; then
    pass "Config shows agent option"
else
    fail "Config missing agent: $CONFIG"
fi

# Test: Default config has no explicit analysis agent (auto-detect)
test_header "Default analysis agent is auto-detect (absent from config)"
reset_config
CONFIG=$($AGR config show)
# agent field is Option<None> by default — should NOT appear in config show
if echo "$CONFIG" | /usr/bin/grep -q '^\s*agent\s*='; then
    fail "Default config should not have explicit agent field: $CONFIG"
else
    pass "Default config has no explicit agent (auto-detect behavior)"
fi

# ============================================
# Analyzer E2E Tests (conditional)
# ============================================

section "Analyzer E2E Tests (conditional)"

# Test: Config validation rejects unknown agent
test_header "Config validation rejects unknown agent"
reset_config
create_config << 'TOMLEOF'
[recording]
auto_analyze = true

[analysis]
agent = "definitely-not-a-real-agent-12345"
TOMLEOF
# Config::load() validates agent names — commands should fail gracefully
OUTPUT=$($AGR config show 2>&1) && EXIT_CODE=0 || EXIT_CODE=$?
if [ "$EXIT_CODE" -ne 0 ] && echo "$OUTPUT" | /usr/bin/grep -qiE "Unknown agent"; then
    pass "Config validation rejects unknown agent name"
else
    fail "Config should reject unknown agent (exit=$EXIT_CODE): $OUTPUT"
fi

# Test: Config with custom analysis agent persists
test_header "Custom analysis agent persists in config"
reset_config
create_config << 'TOMLEOF'
[recording]
auto_analyze = false

[analysis]
agent = "codex"
TOMLEOF
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q 'agent = "codex"'; then
    pass "Custom analysis agent (codex) persists"
else
    fail "Custom analysis agent not persisted: $CONFIG"
fi

# Test: Config with gemini as analysis agent
test_header "Config with gemini analysis agent"
reset_config
create_config << 'TOMLEOF'
[recording]
auto_analyze = true

[analysis]
agent = "gemini"
TOMLEOF
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q 'agent = "gemini"'; then
    pass "gemini as analysis agent accepted"
else
    fail "gemini analysis agent not accepted: $CONFIG"
fi

# Test: Conditional agent detection tests (skip if not installed)
for AGENT in claude codex gemini; do
    test_header "Agent detection for $AGENT"
    if agent_installed "$AGENT"; then
        pass "$AGENT is installed and detected"
    else
        skip "$AGENT not installed (this is OK for CI)"
    fi
done

# Test: Auto-analyze with real agent (conditional)
test_header "Auto-analyze integration (conditional)"
# Try each agent in order of preference
AVAILABLE_AGENT=""
for AGENT in claude codex gemini; do
    if agent_installed "$AGENT"; then
        AVAILABLE_AGENT="$AGENT"
        break
    fi
done

if [ -n "$AVAILABLE_AGENT" ]; then
    echo "  Using available agent: $AVAILABLE_AGENT"
    # Set up config with auto_analyze enabled
    reset_config
    create_config << TOMLEOF
[recording]
auto_analyze = true

[analysis]
agent = "$AVAILABLE_AGENT"
TOMLEOF
    # Note: We don't actually want the agent to analyze in E2E tests
    # as that would be slow and require API keys. We just verify the
    # config is read correctly.
    CONFIG=$($AGR config show)
    if echo "$CONFIG" | /usr/bin/grep -q "auto_analyze = true" && \
       echo "$CONFIG" | /usr/bin/grep -q "agent = \"$AVAILABLE_AGENT\""; then
        pass "Auto-analyze config set correctly for $AVAILABLE_AGENT"
    else
        fail "Auto-analyze config not set correctly: $CONFIG"
    fi
else
    skip "No AI agents installed (claude, codex, gemini) - expected in CI"
fi

# Test: Recording with auto_analyze=false does not trigger analysis
test_header "Recording with auto_analyze=false"
reset_config
create_config << 'TOMLEOF'
[recording]
auto_analyze = false

[analysis]
agent = "claude"
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

# ============================================
# agr analyze Command Tests
# ============================================

section "agr analyze Command Tests"

# Test: agr analyze --help shows usage
test_header "agr analyze --help shows usage"
HELP_OUTPUT=$($AGR analyze --help 2>&1)
if echo "$HELP_OUTPUT" | /usr/bin/grep -q "Analyze a recording" && \
   echo "$HELP_OUTPUT" | /usr/bin/grep -q "\-\-agent"; then
    pass "agr analyze --help shows usage with --agent option"
else
    fail "agr analyze --help missing expected content: $HELP_OUTPUT"
fi

# Test: agr analyze with nonexistent file fails gracefully
test_header "agr analyze nonexistent.cast fails gracefully"
reset_config
OUTPUT=$($AGR analyze /nonexistent/path/to/file.cast 2>&1) && EXIT_CODE=0 || EXIT_CODE=$?
if [ "$EXIT_CODE" -ne 0 ] && echo "$OUTPUT" | /usr/bin/grep -qiE "not found|no such file"; then
    pass "agr analyze fails gracefully with nonexistent file"
else
    fail "agr analyze should fail with nonexistent file (exit=$EXIT_CODE): $OUTPUT"
fi

# Test: agr analyze with missing agent fails gracefully
test_header "agr analyze with missing agent fails gracefully"
reset_config
# First create a valid cast file
$AGR record echo -- "test" </dev/null 2>&1
CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/tail -1)
if [ -f "$CAST_FILE" ]; then
    OUTPUT=$($AGR analyze "$CAST_FILE" --agent definitely-not-a-real-agent-12345 2>&1) && EXIT_CODE=0 || EXIT_CODE=$?
    if [ "$EXIT_CODE" -ne 0 ] && echo "$OUTPUT" | /usr/bin/grep -qi "Unknown agent"; then
        pass "agr analyze fails gracefully with unknown agent"
    else
        fail "agr analyze should fail with missing agent (exit=$EXIT_CODE): $OUTPUT"
    fi
else
    skip "Could not create test cast file for analyze test"
fi

# Test: agr analyze uses default agent from config
test_header "agr analyze uses default agent from config"
reset_config
create_config << 'TOMLEOF'
[recording]
auto_analyze = false

[analysis]
agent = "claude"
TOMLEOF
# Create a valid cast file if not already present
$AGR record echo -- "test default agent" </dev/null 2>&1
CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/tail -1)
if [ -f "$CAST_FILE" ]; then
    # Fake claude is in PATH, so this should succeed with the configured agent
    OUTPUT=$($AGR analyze "$CAST_FILE" 2>&1) && EXIT_CODE=0 || EXIT_CODE=$?
    if [ "$EXIT_CODE" -eq 0 ] && echo "$OUTPUT" | /usr/bin/grep -qi "marker"; then
        pass "agr analyze uses config's analysis agent (claude)"
    else
        fail "agr analyze should use config's analysis agent (exit=$EXIT_CODE): $OUTPUT"
    fi
else
    skip "Could not create test cast file for default agent test"
fi

# Test: agr analyze --agent overrides config
test_header "agr analyze --agent overrides config"
reset_config
create_config << 'TOMLEOF'
[recording]
auto_analyze = false

[analysis]
agent = "claude"
TOMLEOF
CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/tail -1)
if [ -f "$CAST_FILE" ]; then
    OUTPUT=$($AGR analyze "$CAST_FILE" --agent override-agent-test 2>&1) && EXIT_CODE=0 || EXIT_CODE=$?
    # Should fail because override-agent-test is not a supported agent
    if [ "$EXIT_CODE" -ne 0 ] && echo "$OUTPUT" | /usr/bin/grep -qi "Unknown agent"; then
        pass "agr analyze --agent successfully overrides config with unknown agent"
    else
        fail "agr analyze --agent should override config (exit=$EXIT_CODE): $OUTPUT"
    fi
else
    skip "Could not create test cast file for agent override test"
fi

# Clean up test config
reset_config

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
