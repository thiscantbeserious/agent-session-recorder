#!/bin/bash
# Completions tests for AGR
# Tests: completions command, cast file listing, short path resolution

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    check_prerequisites || exit 1
    section "AGR Completions Tests"
    echo "Test directory: $TEST_DIR"
fi

# ============================================
# Completions Command Tests
# ============================================

section "Completions Command Tests"

# Test: completions --shell bash generates output
test_header "completions --shell bash generates output"
BASH_OUTPUT=$($AGR completions --shell bash 2>&1)
if echo "$BASH_OUTPUT" | /usr/bin/grep -q "complete"; then
    pass "completions --shell bash generates bash completion script"
else
    fail "completions --shell bash did not generate completion script"
fi

# Test: completions --shell zsh generates output
test_header "completions --shell zsh generates output"
ZSH_OUTPUT=$($AGR completions --shell zsh 2>&1)
if echo "$ZSH_OUTPUT" | /usr/bin/grep -q "#compdef"; then
    pass "completions --shell zsh generates zsh completion script"
else
    fail "completions --shell zsh did not generate completion script"
fi

# Test: completions --files lists cast files
test_header "completions --files lists cast files"
# First create some test sessions
mkdir -p "$HOME/recorded_agent_sessions/test-agent"
echo '{"version": 3}' > "$HOME/recorded_agent_sessions/test-agent/session1.cast"
echo '{"version": 3}' > "$HOME/recorded_agent_sessions/test-agent/session2.cast"
mkdir -p "$HOME/recorded_agent_sessions/other-agent"
echo '{"version": 3}' > "$HOME/recorded_agent_sessions/other-agent/session3.cast"

FILES_OUTPUT=$($AGR completions --files 2>&1)
if echo "$FILES_OUTPUT" | /usr/bin/grep -q "test-agent/session1.cast" && \
   echo "$FILES_OUTPUT" | /usr/bin/grep -q "test-agent/session2.cast" && \
   echo "$FILES_OUTPUT" | /usr/bin/grep -q "other-agent/session3.cast"; then
    pass "completions --files lists all cast files in agent/file.cast format"
else
    fail "completions --files did not list expected files: $FILES_OUTPUT"
fi

# Test: completions --files with prefix filters results
test_header "completions --files with prefix filters results"
FILTERED_OUTPUT=$($AGR completions --files "test-agent/" 2>&1)
if echo "$FILTERED_OUTPUT" | /usr/bin/grep -q "test-agent/session1.cast" && \
   ! echo "$FILTERED_OUTPUT" | /usr/bin/grep -q "other-agent/session3.cast"; then
    pass "completions --files with prefix filters correctly"
else
    fail "completions --files prefix filtering failed: $FILTERED_OUTPUT"
fi

# Test: completions with no args shows usage
test_header "completions with no args shows usage"
NO_ARGS_OUTPUT=$($AGR completions 2>&1 || true)
if echo "$NO_ARGS_OUTPUT" | /usr/bin/grep -qi "usage"; then
    pass "completions with no args shows usage"
else
    fail "completions with no args did not show usage: $NO_ARGS_OUTPUT"
fi

# ============================================
# Short Path Resolution Tests
# ============================================

section "Short Path Resolution Tests"

# Test: marker list with short path
test_header "marker list with short path (agent/file.cast format)"
# The file needs to be a valid cast file for marker list to work
echo '{"version": 3}' > "$HOME/recorded_agent_sessions/test-agent/markers-test.cast"

# marker list should work with short path
MARKER_OUTPUT=$($AGR marker list test-agent/markers-test.cast 2>&1)
if echo "$MARKER_OUTPUT" | /usr/bin/grep -qiE "(no markers|markers:)"; then
    pass "marker list works with short path format"
else
    fail "marker list with short path failed: $MARKER_OUTPUT"
fi

# Test: marker add with short path
test_header "marker add with short path"
# Create a valid asciicast file with header
cat > "$HOME/recorded_agent_sessions/test-agent/marker-add-test.cast" << 'EOF'
{"version": 3, "width": 80, "height": 24, "timestamp": 1234567890}
[0.0, "o", "hello"]
EOF

ADD_OUTPUT=$($AGR marker add test-agent/marker-add-test.cast 1.0 "Test marker" 2>&1)
if echo "$ADD_OUTPUT" | /usr/bin/grep -q "Marker added"; then
    pass "marker add works with short path format"
else
    fail "marker add with short path failed: $ADD_OUTPUT"
fi

# ============================================
# Shell Install Completions Tests
# ============================================

section "Shell Install Completions Tests"

# Test: shell install creates completion files
test_header "shell install creates completion files"
# First ensure clean state
rm -f "$HOME/.zshrc" "$HOME/.bashrc"
rm -rf "$HOME/.config/agr" "$HOME/.local/share/bash-completion" "$HOME/.zsh/completions"

# Create rc file for install
touch "$HOME/.zshrc"

# Run shell install
$AGR shell install 2>&1

# Check that completion files were created
BASH_COMP_PATH="$HOME/.local/share/bash-completion/completions/agr"
ZSH_COMP_PATH="$HOME/.zsh/completions/_agr"

if [ -f "$BASH_COMP_PATH" ] && [ -f "$ZSH_COMP_PATH" ]; then
    pass "shell install creates both completion files"
else
    if [ ! -f "$BASH_COMP_PATH" ]; then
        fail "shell install did not create bash completion at $BASH_COMP_PATH"
    fi
    if [ ! -f "$ZSH_COMP_PATH" ]; then
        fail "shell install did not create zsh completion at $ZSH_COMP_PATH"
    fi
fi

# Test: bash completion file contains expected content
test_header "bash completion file has expected content"
if [ -f "$BASH_COMP_PATH" ] && /usr/bin/grep -q "_agr_complete_files" "$BASH_COMP_PATH"; then
    pass "bash completion file contains expected functions"
else
    fail "bash completion file missing expected content"
fi

# Test: zsh completion file contains expected content
test_header "zsh completion file has expected content"
if [ -f "$ZSH_COMP_PATH" ] && /usr/bin/grep -q "#compdef agr" "$ZSH_COMP_PATH"; then
    pass "zsh completion file contains expected header"
else
    fail "zsh completion file missing expected content"
fi

# Test: shell uninstall removes completion files
test_header "shell uninstall removes completion files"
$AGR shell uninstall 2>&1

if [ ! -f "$BASH_COMP_PATH" ] && [ ! -f "$ZSH_COMP_PATH" ]; then
    pass "shell uninstall removes both completion files"
else
    if [ -f "$BASH_COMP_PATH" ]; then
        fail "shell uninstall did not remove bash completion"
    fi
    if [ -f "$ZSH_COMP_PATH" ]; then
        fail "shell uninstall did not remove zsh completion"
    fi
fi

# ============================================
# agr.sh Completion Setup Tests
# ============================================

section "agr.sh Completion Setup Tests"

# Test: agr.sh contains completion setup function
test_header "agr.sh contains _agr_setup_completions function"
# Re-install to get the shell script
touch "$HOME/.zshrc"
$AGR shell install 2>&1

AGR_SH="$HOME/.config/agr/agr.sh"
if [ -f "$AGR_SH" ] && /usr/bin/grep -q "_agr_setup_completions" "$AGR_SH"; then
    pass "agr.sh contains _agr_setup_completions function"
else
    fail "agr.sh missing _agr_setup_completions function"
fi

# Clean up for next test suite
$AGR shell uninstall 2>/dev/null || true

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
