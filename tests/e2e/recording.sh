#!/bin/bash
# Recording, list, and status tests for AGR
# Tests: record command, list command, status command, cast file structure

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    check_prerequisites || exit 1
    section "AGR Recording Tests"
    echo "Test directory: $TEST_DIR"
    # Create CI config when running standalone (main runner does this otherwise)
    create_ci_config
fi

# Test: Record a simple command
test_header "Record simple command"
$AGR record echo -- "hello e2e test" </dev/null
CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/head -1)
if [ -f "$CAST_FILE" ]; then
    pass "Recording created file"
else
    fail "Recording did not create file"
fi

# Test: Verify cast file is valid asciicast v3
test_header "Verify asciicast v3 format"
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

# Test: Verify output was captured
test_header "Verify output captured"
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

# Test: List shows the recording
test_header "List command shows recording"
LIST_OUTPUT=$($AGR list)
if echo "$LIST_OUTPUT" | /usr/bin/grep -q "echo"; then
    pass "List command shows echo agent recording"
else
    fail "List command missing recording: $LIST_OUTPUT"
fi

# Test: Status shows correct count
test_header "Status shows correct count"
STATUS_OUTPUT=$($AGR status)
if echo "$STATUS_OUTPUT" | /usr/bin/grep -q "1 total"; then
    pass "Status shows 1 session"
else
    fail "Status count incorrect: $STATUS_OUTPUT"
fi

# Test: Record with different agent
test_header "Record with different agent"
$AGR record ls -- -la </dev/null
LS_CAST=$(ls "$HOME/recorded_agent_sessions/ls/"*.cast 2>/dev/null | /usr/bin/head -1)
if [ -f "$LS_CAST" ]; then
    pass "Second recording with different agent created"
else
    fail "Second recording not created"
fi

# Test: Record with --name flag (skips rename prompt)
test_header "Record with --name flag"
$AGR record echo --name "my-custom-session" -- "test with name flag" </dev/null
NAMED_CAST="$HOME/recorded_agent_sessions/echo/my-custom-session.cast"
if [ -f "$NAMED_CAST" ]; then
    pass "Recording with --name flag created correct filename"
else
    fail "Recording with --name flag did not create expected file"
    ls "$HOME/recorded_agent_sessions/echo/"
fi

# Test: List filter by agent
test_header "List filter by agent"
ECHO_LIST=$($AGR list echo)
LS_LIST=$($AGR list ls)
if echo "$ECHO_LIST" | /usr/bin/grep -q "echo" && ! echo "$ECHO_LIST" | /usr/bin/grep -q "\.cast (ls,"; then
    pass "List filters by agent correctly"
else
    fail "List filter not working: echo=$ECHO_LIST"
fi

# Test: Status shows multiple agents
test_header "Status with multiple agents"
STATUS=$($AGR status)
if echo "$STATUS" | /usr/bin/grep -q "3 total" && echo "$STATUS" | /usr/bin/grep -q "echo:" && echo "$STATUS" | /usr/bin/grep -q "ls:"; then
    pass "Status shows both agents"
else
    fail "Status not showing both agents: $STATUS"
fi

# Test: Cast file has proper events structure
test_header "Cast file event structure"
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

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
