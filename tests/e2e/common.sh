#!/bin/bash
# Common setup, helpers, and cleanup for AGR E2E tests
# This file should be sourced by all category test files

# Prevent multiple sourcing
if [ -n "$_AGR_E2E_COMMON_SOURCED" ]; then
    return 0
fi
_AGR_E2E_COMMON_SOURCED=1

# Enable strict mode
set -e

# Determine paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TESTS_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_DIR="$(dirname "$TESTS_DIR")"
AGR="$PROJECT_DIR/target/release/agr"

# Test directory setup - only create once
if [ -z "$_AGR_TEST_DIR_CREATED" ]; then
    TEST_DIR=$(mktemp -d)
    ORIGINAL_HOME="$HOME"
    export HOME="$TEST_DIR"
    mkdir -p "$HOME/recorded_agent_sessions"
    _AGR_TEST_DIR_CREATED=1
fi

# Cleanup function
cleanup() {
    export HOME="$ORIGINAL_HOME"
    rm -rf "$TEST_DIR"
}

# Only set trap if we're the main runner or standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    trap cleanup EXIT
fi

# Test counters - initialize only if not already set (for main runner aggregation)
if [ -z "$_AGR_COUNTERS_INITIALIZED" ]; then
    PASS=0
    FAIL=0
    _AGR_COUNTERS_INITIALIZED=1
fi

# Helper functions
pass() {
    echo "  PASS: $1"
    PASS=$((PASS + 1))
}

fail() {
    echo "  FAIL: $1"
    FAIL=$((FAIL + 1))
}

skip() {
    echo "  SKIP: $1"
    # Count skips as passes (valid behavior)
    PASS=$((PASS + 1))
}

# Check prerequisites
check_prerequisites() {
    local errors=0

    if ! command -v asciinema &>/dev/null; then
        echo "ERROR: asciinema not installed"
        errors=1
    fi

    if [ ! -x "$AGR" ]; then
        echo "ERROR: AGR binary not found at $AGR"
        echo "Run 'cargo build --release' first"
        errors=1
    fi

    return $errors
}

# Helper to check if an agent binary is available
agent_installed() {
    local AGENT=$1
    command -v "$AGENT" &>/dev/null
}

# Reset config for clean test state
reset_config() {
    rm -f "$HOME/.config/agr/config.toml"
}

# Create config with specific content
create_config() {
    mkdir -p "$HOME/.config/agr"
    cat > "$HOME/.config/agr/config.toml"
}

# Print section header
section() {
    echo
    echo "=== $1 ==="
}

# Print test header
test_header() {
    echo "--- $1 ---"
}

# Print test summary (for standalone runs)
print_summary() {
    echo
    echo "=== Test Summary ==="
    echo "Passed: $PASS"
    echo "Failed: $FAIL"
    echo

    if [ $FAIL -gt 0 ]; then
        return 1
    fi
    return 0
}

# Export variables for subshells
export TEST_DIR ORIGINAL_HOME AGR PROJECT_DIR
