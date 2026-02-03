#!/bin/bash
# End-to-end tests for AGR with real asciinema
# Main test runner - calls all category test files

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
E2E_DIR="$SCRIPT_DIR/e2e"

# Mark this as the main runner so category files don't set up their own traps
export _AGR_E2E_MAIN_RUNNER=1

# Source common setup - this creates TEST_DIR, sets HOME, etc.
source "$E2E_DIR/common.sh"

# Check prerequisites before running any tests
if ! check_prerequisites; then
    exit 1
fi

echo "=== AGR End-to-End Tests ==="
echo "Test directory: $TEST_DIR"
echo

# Set the cleanup trap after common.sh has set up TEST_DIR
trap cleanup EXIT

# Run each category of tests in order
# Order matters - some tests depend on state from previous tests

echo "Running recording tests..."
source "$E2E_DIR/recording.sh"

echo
echo "Running marker tests..."
source "$E2E_DIR/markers.sh"

echo
echo "Running cleanup tests..."
source "$E2E_DIR/cleanup.sh"

echo
echo "Running agent configuration tests..."
source "$E2E_DIR/agents.sh"

echo
echo "Running shell integration tests..."
source "$E2E_DIR/shell.sh"

echo
echo "Running analyzer configuration tests..."
source "$E2E_DIR/analyzer.sh"

echo
echo "Running completions tests..."
source "$E2E_DIR/completions.sh"

echo
echo "Running clipboard tests..."
source "$E2E_DIR/clipboard.sh"

# Print final summary
echo
echo "=== Test Summary ==="
echo "Passed: $PASS"
echo "Failed: $FAIL"
echo

if [ $FAIL -gt 0 ]; then
    exit 1
fi
echo "All e2e tests passed!"
