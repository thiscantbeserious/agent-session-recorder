#!/bin/bash
# TUI key press tests for AGR
# Tests that interactive TUI commands (ls, cleanup) handle key presses correctly.
# Requires: expect (for PTY-based keystroke injection)
#
# These tests guard against regressions like the event-loop crash in #146
# where any keypress caused "Event channel closed: receiving on a closed channel".

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [[ -z "$_AGR_E2E_MAIN_RUNNER" ]]; then
    check_prerequisites || exit 1
    section "AGR TUI Key Press Tests"
    echo "Test directory: $TEST_DIR"
    create_ci_config
fi

# Check if expect is available (required for PTY-based TUI testing)
if ! command -v expect &>/dev/null; then
    skip "expect not installed — skipping all TUI key press tests"
    if [[ -z "$_AGR_E2E_MAIN_RUNNER" ]]; then
        print_summary
        exit $?
    fi
    return 0 2>/dev/null || exit 0
fi

section "TUI Key Press Tests"

# Ensure recordings exist for TUI to display
EXISTING_CASTS=$(find "$HOME/recorded_agent_sessions" -name "*.cast" 2>/dev/null | wc -l | tr -d ' ')
if [[ "$EXISTING_CASTS" -lt 2 ]]; then
    $AGR record echo -- "tui test session 1" </dev/null 2>/dev/null || true
    $AGR record echo -- "tui test session 2" </dev/null 2>/dev/null || true
fi

# Helper: Run a TUI command via expect, sending keystrokes after startup.
# Usage: run_tui_expect <command> <expect_send_script> [timeout_seconds]
#
# Returns the exit code of the spawned process via a temp file (to work with set -e).
_TUI_EXIT_FILE=$(mktemp)
run_tui_expect() {
    local cmd="$1"
    local send_script="$2"
    local timeout_sec="${3:-5}"

    local output
    output=$(expect -c "
        log_user 0
        set timeout $timeout_sec
        spawn env HOME=$HOME TERM=xterm $cmd
        sleep 0.5
        $send_script
        expect {
            eof {}
            timeout { exit 124 }
        }
        catch wait result
        exit [lindex \$result 3]
    " 2>&1) && echo "0" > "$_TUI_EXIT_FILE" || echo "$?" > "$_TUI_EXIT_FILE"
    echo "$output"
}

tui_exit_code() {
    cat "$_TUI_EXIT_FILE"
}

# ============================================
# agr list — key press tests
# ============================================

# Test: agr list exits cleanly on 'q'
test_header "agr list exits cleanly on 'q' keypress"
OUTPUT=$(run_tui_expect "$AGR list" 'send "q"')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr list exits cleanly on 'q'"
else
    fail "agr list failed on 'q' (exit=$(tui_exit_code))"
fi

# Test: agr list handles arrow key navigation without crashing
test_header "agr list handles arrow keys then 'q'"
OUTPUT=$(run_tui_expect "$AGR list" '
    send "\033\[B"
    sleep 0.1
    send "\033\[A"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr list handles arrow keys without crashing"
else
    fail "agr list crashed on arrow keys (exit=$(tui_exit_code))"
fi

# Test: agr list handles j/k vim navigation
test_header "agr list handles j/k navigation then 'q'"
OUTPUT=$(run_tui_expect "$AGR list" '
    send "j"
    sleep 0.1
    send "k"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr list handles j/k navigation without crashing"
else
    fail "agr list crashed on j/k navigation (exit=$(tui_exit_code))"
fi

# Test: agr list handles search mode (/ then Esc)
test_header "agr list handles search mode entry and exit"
OUTPUT=$(run_tui_expect "$AGR list" '
    send "/"
    sleep 0.1
    send "test"
    sleep 0.1
    send "\033"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr list handles search mode without crashing"
else
    fail "agr list crashed in search mode (exit=$(tui_exit_code))"
fi

# Test: agr list handles help mode (? to open, q to close help, q to quit)
test_header "agr list handles help mode"
OUTPUT=$(run_tui_expect "$AGR list" '
    send "?"
    sleep 0.3
    send "q"
    sleep 0.2
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr list handles help mode without crashing"
else
    fail "agr list crashed in help mode (exit=$(tui_exit_code))"
fi

# Test: agr list handles Ctrl+C
test_header "agr list handles Ctrl+C"
OUTPUT=$(run_tui_expect "$AGR list" 'send "\003"')
# Ctrl+C may exit with 0 or signal code — the key test is no "Event channel closed" error
if echo "$OUTPUT" | grep -q "Event channel closed"; then
    fail "agr list crashed on Ctrl+C with event channel error"
else
    pass "agr list handles Ctrl+C without event channel error"
fi

# Test: agr list handles Escape key
test_header "agr list handles Escape then 'q'"
OUTPUT=$(run_tui_expect "$AGR list" '
    send "\033"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr list handles Escape key without crashing"
else
    fail "agr list crashed on Escape (exit=$(tui_exit_code))"
fi

# Test: agr list handles agent filter mode (f then Esc)
test_header "agr list handles agent filter mode"
OUTPUT=$(run_tui_expect "$AGR list" '
    send "f"
    sleep 0.3
    send "\033"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr list handles agent filter mode without crashing"
else
    fail "agr list crashed in agent filter mode (exit=$(tui_exit_code))"
fi

# ============================================
# agr cleanup — key press tests
# ============================================

# Test: agr cleanup exits cleanly on 'q'
test_header "agr cleanup exits cleanly on 'q' keypress"
OUTPUT=$(run_tui_expect "$AGR cleanup" 'send "q"')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr cleanup exits cleanly on 'q'"
else
    fail "agr cleanup failed on 'q' (exit=$(tui_exit_code))"
fi

# Test: agr cleanup handles arrow keys
test_header "agr cleanup handles arrow keys then 'q'"
OUTPUT=$(run_tui_expect "$AGR cleanup" '
    send "\033\[B"
    sleep 0.1
    send "\033\[A"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr cleanup handles arrow keys without crashing"
else
    fail "agr cleanup crashed on arrow keys (exit=$(tui_exit_code))"
fi

# Test: agr cleanup handles space (toggle select) then 'q'
test_header "agr cleanup handles space toggle then 'q'"
OUTPUT=$(run_tui_expect "$AGR cleanup" '
    send " "
    sleep 0.1
    send " "
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr cleanup handles space toggle without crashing"
else
    fail "agr cleanup crashed on space toggle (exit=$(tui_exit_code))"
fi

# Test: agr cleanup handles 'a' (select all) then 'q'
test_header "agr cleanup handles select all then 'q'"
OUTPUT=$(run_tui_expect "$AGR cleanup" '
    send "a"
    sleep 0.1
    send "a"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr cleanup handles select all without crashing"
else
    fail "agr cleanup crashed on select all (exit=$(tui_exit_code))"
fi

# Test: agr cleanup handles Ctrl+C
test_header "agr cleanup handles Ctrl+C"
OUTPUT=$(run_tui_expect "$AGR cleanup" 'send "\003"')
if echo "$OUTPUT" | grep -q "Event channel closed"; then
    fail "agr cleanup crashed on Ctrl+C with event channel error"
else
    pass "agr cleanup handles Ctrl+C without event channel error"
fi

# Test: agr cleanup handles search mode
test_header "agr cleanup handles search mode"
OUTPUT=$(run_tui_expect "$AGR cleanup" '
    send "/"
    sleep 0.1
    send "test"
    sleep 0.1
    send "\033"
    sleep 0.1
    send "q"
')
if [[ "$(tui_exit_code)" -eq 0 ]]; then
    pass "agr cleanup handles search mode without crashing"
else
    fail "agr cleanup crashed in search mode (exit=$(tui_exit_code))"
fi

# Clean up
rm -f "$_TUI_EXIT_FILE"

# Print summary when running standalone
if [[ -z "$_AGR_E2E_MAIN_RUNNER" ]]; then
    print_summary
    exit $?
fi
