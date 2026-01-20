#!/bin/bash
# Agent configuration tests for AGR
# Tests: agents add/remove, agents list, agents no-wrap, agents is-wrapped

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    check_prerequisites || exit 1
    section "AGR Agent Configuration Tests"
    echo "Test directory: $TEST_DIR"
fi

# Test: Agents add/remove
test_header "Agents add/remove"
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

# Test: Config persistence
test_header "Config persistence"
$AGR agents add persistent-agent
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "persistent-agent"; then
    pass "Config persists agent addition"
else
    fail "Config not persisted: $CONFIG"
fi

# Test: Agents no-wrap list (empty by default)
test_header "Agents no-wrap list (empty)"
# Reset config to ensure clean state
reset_config
NOWRAP_OUTPUT=$($AGR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "No agents in no-wrap list"; then
    pass "No-wrap list empty by default"
else
    fail "No-wrap list not empty: $NOWRAP_OUTPUT"
fi

# Test: Agents no-wrap add
test_header "Agents no-wrap add"
$AGR agents no-wrap add test-nowrap-agent
NOWRAP_OUTPUT=$($AGR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "test-nowrap-agent"; then
    pass "Agent added to no-wrap list"
else
    fail "Agent not in no-wrap list: $NOWRAP_OUTPUT"
fi

# Test: Agents is-wrapped (agent in no-wrap should return exit 1)
test_header "Agents is-wrapped respects no-wrap list"
if $AGR agents is-wrapped test-nowrap-agent 2>/dev/null; then
    fail "is-wrapped returned 0 for agent in no-wrap list"
else
    pass "is-wrapped correctly returns 1 for agent in no-wrap list"
fi

# Test: Agents is-wrapped (enabled agent should return exit 0)
test_header "Agents is-wrapped for enabled agent"
$AGR agents add wrap-test-agent
if $AGR agents is-wrapped wrap-test-agent 2>/dev/null; then
    pass "is-wrapped returns 0 for enabled agent"
else
    fail "is-wrapped returned 1 for enabled agent"
fi

# Test: Agents no-wrap remove
test_header "Agents no-wrap remove"
$AGR agents no-wrap remove test-nowrap-agent
NOWRAP_OUTPUT=$($AGR agents no-wrap list 2>&1)
if echo "$NOWRAP_OUTPUT" | /usr/bin/grep -q "No agents in no-wrap list"; then
    pass "Agent removed from no-wrap list"
else
    fail "Agent still in no-wrap list: $NOWRAP_OUTPUT"
fi

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
