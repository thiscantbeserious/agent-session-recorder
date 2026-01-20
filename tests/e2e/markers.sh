#!/bin/bash
# Marker tests for AGR
# Tests: marker add, marker list

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    check_prerequisites || exit 1
    section "AGR Marker Tests"
    echo "Test directory: $TEST_DIR"
fi

# Ensure we have a recording to work with
CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/head -1)
if [ -z "$CAST_FILE" ] || [ ! -f "$CAST_FILE" ]; then
    # Create a recording first
    $AGR record echo -- "marker test recording" </dev/null
    CAST_FILE=$(ls "$HOME/recorded_agent_sessions/echo/"*.cast 2>/dev/null | /usr/bin/head -1)
fi

# Test: Add marker to recording
test_header "Add marker to recording"
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

# Test: List markers shows the marker
test_header "List markers"
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

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
