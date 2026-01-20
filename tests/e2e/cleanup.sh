#!/bin/bash
# Cleanup command tests for AGR
# Tests: cleanup command (non-interactive)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    check_prerequisites || exit 1
    section "AGR Cleanup Tests"
    echo "Test directory: $TEST_DIR"
fi

# Ensure we have at least one recording for cleanup to find
if [ ! -d "$HOME/recorded_agent_sessions" ] || [ -z "$(ls -A "$HOME/recorded_agent_sessions" 2>/dev/null)" ]; then
    # Create a recording first
    $AGR record echo -- "cleanup test recording" </dev/null
fi

# Count current sessions
SESSION_COUNT=$(find "$HOME/recorded_agent_sessions" -name "*.cast" 2>/dev/null | wc -l | tr -d ' ')

# Test: Cleanup (non-interactive)
test_header "Cleanup command"
CLEANUP_OUTPUT=$(echo "0" | $AGR cleanup)
if echo "$CLEANUP_OUTPUT" | /usr/bin/grep -qE "Found [0-9]+ sessions"; then
    pass "Cleanup finds sessions"
else
    fail "Cleanup output unexpected: $CLEANUP_OUTPUT"
fi

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
