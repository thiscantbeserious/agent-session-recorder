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

# Test: shell install embeds completions in RC file
test_header "shell install embeds completions in RC file"
# First ensure clean state
rm -f "$HOME/.zshrc" "$HOME/.bashrc"
rm -rf "$HOME/.config/agr" "$HOME/.local/share/bash-completion" "$HOME/.zsh/completions"

# Create rc file for install
touch "$HOME/.zshrc"

# Run shell install
$AGR shell install 2>&1

# Completions are now embedded directly in the RC file (no separate files)
# Check that the RC file contains the completion function
if /usr/bin/grep -q "_agr_complete" "$HOME/.zshrc"; then
    pass "shell install embeds completion function in RC file"
else
    fail "shell install did not embed completion function in RC file"
fi

# Test: embedded completions contain command list
test_header "embedded completions contain command list"
if /usr/bin/grep -q "_agr_commands" "$HOME/.zshrc"; then
    pass "embedded completions contain command list"
else
    fail "embedded completions missing command list"
fi

# Test: embedded completions contain menu select styling
test_header "embedded completions contain menu select styling"
if /usr/bin/grep -q "menu select" "$HOME/.zshrc"; then
    pass "embedded completions contain menu select styling"
else
    fail "embedded completions missing menu select styling"
fi

# Test: shell uninstall cleans up old completion files if they exist
test_header "shell uninstall cleans up properly"
# Create fake old-style completion files to test cleanup
BASH_COMP_PATH="$HOME/.local/share/bash-completion/completions/agr"
ZSH_COMP_PATH="$HOME/.zsh/completions/_agr"
mkdir -p "$(dirname "$BASH_COMP_PATH")" "$(dirname "$ZSH_COMP_PATH")"
touch "$BASH_COMP_PATH" "$ZSH_COMP_PATH"

$AGR shell uninstall 2>&1

# Old completion files should be removed, and RC file should not contain AGR markers
if [ ! -f "$BASH_COMP_PATH" ] && [ ! -f "$ZSH_COMP_PATH" ] && ! /usr/bin/grep -q "AGR" "$HOME/.zshrc"; then
    pass "shell uninstall cleans up old files and RC markers"
else
    fail "shell uninstall did not clean up properly"
fi

# ============================================
# agr.sh Completion Setup Tests
# ============================================

section "agr.sh Completion Setup Tests"

# Test: Embedded script contains dynamic completion function
test_header "Embedded script contains _agr_complete function"
# Re-install to get the shell integration (script is now embedded in .zshrc)
touch "$HOME/.zshrc"
$AGR shell install 2>&1

# The script is now embedded directly in .zshrc with dynamic completions
# Check for the completion function generated from clap definitions
if /usr/bin/grep -q "_agr_complete" "$HOME/.zshrc"; then
    pass "Embedded script contains _agr_complete function"
else
    fail "Embedded script missing _agr_complete function"
fi

# Clean up for next test suite
$AGR shell uninstall 2>/dev/null || true

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
