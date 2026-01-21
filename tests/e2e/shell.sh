#!/bin/bash
# Shell integration tests for AGR
# Tests: shell install/uninstall, shell status, wrapper functions

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

# Check prerequisites when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    check_prerequisites || exit 1
    section "AGR Shell Integration Tests"
    echo "Test directory: $TEST_DIR"
fi

# Test: Shell status (before install)
test_header "Shell status (before install)"
# Reset config and shell files for clean state
reset_config
rm -f "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.config/agr/agr.sh"
SHELL_OUTPUT=$($AGR shell status 2>&1)
if echo "$SHELL_OUTPUT" | /usr/bin/grep -q "not installed"; then
    pass "Shell status shows not installed"
else
    fail "Shell status unexpected output: $SHELL_OUTPUT"
fi

# Test: Shell install
test_header "Shell install"
# Create a .zshrc for testing
touch "$HOME/.zshrc"
$AGR shell install
if /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell install added marked section to .zshrc"
else
    fail "Shell install did not modify .zshrc"
    cat "$HOME/.zshrc"
fi
if [ -f "$HOME/.config/agr/agr.sh" ]; then
    pass "Shell install created agr.sh script"
else
    fail "Shell install did not create agr.sh"
fi

# Test: Config.toml created during shell install
test_header "Config.toml created during shell install"
if [ -f "$HOME/.config/agr/config.toml" ]; then
    pass "Shell install created config.toml"
else
    fail "Shell install did not create config.toml"
fi

# Test: Default agents in config.toml
test_header "Default agents in config.toml"
if /usr/bin/grep -q '"claude"' "$HOME/.config/agr/config.toml" && \
   /usr/bin/grep -q '"codex"' "$HOME/.config/agr/config.toml" && \
   /usr/bin/grep -q '"gemini"' "$HOME/.config/agr/config.toml"; then
    pass "Config.toml contains default agents (claude, codex, gemini)"
else
    fail "Config.toml missing expected default agents"
    cat "$HOME/.config/agr/config.toml"
fi

# Test: Config.toml does NOT contain gemini-cli (old name)
test_header "Config.toml uses gemini not gemini-cli"
if ! /usr/bin/grep -q 'gemini-cli' "$HOME/.config/agr/config.toml"; then
    pass "Config.toml correctly uses 'gemini' not 'gemini-cli'"
else
    fail "Config.toml contains deprecated 'gemini-cli'"
    cat "$HOME/.config/agr/config.toml"
fi

# Test: agr agents list shows correct defaults
test_header "agr agents list shows correct defaults"
AGENTS_OUTPUT=$($AGR agents list 2>&1)
if echo "$AGENTS_OUTPUT" | /usr/bin/grep -q "claude" && \
   echo "$AGENTS_OUTPUT" | /usr/bin/grep -q "codex" && \
   echo "$AGENTS_OUTPUT" | /usr/bin/grep -q "gemini"; then
    pass "agr agents list shows claude, codex, gemini"
else
    fail "agr agents list missing expected agents: $AGENTS_OUTPUT"
fi

# Test: agr agents list does NOT show gemini-cli
test_header "agr agents list uses gemini not gemini-cli"
if ! echo "$AGENTS_OUTPUT" | /usr/bin/grep -q "gemini-cli"; then
    pass "agr agents list correctly shows 'gemini' not 'gemini-cli'"
else
    fail "agr agents list shows deprecated 'gemini-cli': $AGENTS_OUTPUT"
fi

# Test: Shell status (after install)
test_header "Shell status (after install)"
SHELL_OUTPUT=$($AGR shell status 2>&1)
if echo "$SHELL_OUTPUT" | /usr/bin/grep -qiE '\binstalled\b' \
   && ! echo "$SHELL_OUTPUT" | /usr/bin/grep -qiE '\bnot installed\b'; then
    pass "Shell status shows installed"
else
    fail "Shell status not showing installed: $SHELL_OUTPUT"
fi

# Test: Shell uninstall
test_header "Shell uninstall"
$AGR shell uninstall
if ! /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell uninstall removed marked section from .zshrc"
else
    fail "Shell uninstall did not clean .zshrc"
    cat "$HOME/.zshrc"
fi

# Test: Auto-wrap config toggle
test_header "Auto-wrap config toggle"
CONFIG=$($AGR config show)
if echo "$CONFIG" | /usr/bin/grep -q "auto_wrap"; then
    pass "Config shows auto_wrap setting"
else
    fail "Config missing auto_wrap: $CONFIG"
fi

# Test: Shell install with existing content preserves it
test_header "Shell install preserves existing .zshrc content"
echo "# My existing config" > "$HOME/.zshrc"
echo "export MY_VAR=test" >> "$HOME/.zshrc"
$AGR shell install
if /usr/bin/grep -q "MY_VAR=test" "$HOME/.zshrc" && /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell install preserved existing content"
else
    fail "Shell install did not preserve existing content"
    cat "$HOME/.zshrc"
fi

# Test: Shell uninstall preserves other content
test_header "Shell uninstall preserves other content"
$AGR shell uninstall
if /usr/bin/grep -q "MY_VAR=test" "$HOME/.zshrc" && ! /usr/bin/grep -q "AGR (Agent Session Recorder)" "$HOME/.zshrc"; then
    pass "Shell uninstall preserved other content"
else
    fail "Shell uninstall did not preserve other content"
    cat "$HOME/.zshrc"
fi

# ============================================
# NEW: Shell Wrapper Function Tests
# ============================================

section "Shell Wrapper Function Tests"

# Helper: Source agr.sh in a subshell to test wrapper functions
test_agr_sh() {
    local test_script="$1"
    # Run in a subshell with a clean environment
    (
        # Set up minimal environment
        export PATH="$PROJECT_DIR/target/release:$PATH"
        export HOME="$TEST_DIR"

        # Source the agr.sh
        AGR_SH="$HOME/.config/agr/agr.sh"
        if [ -f "$AGR_SH" ]; then
            # Use bash to source and run test
            bash -c "source '$AGR_SH'; $test_script"
            return $?
        else
            return 1
        fi
    )
    return $?
}

# Re-install shell integration for wrapper tests
$AGR shell install

# Test: agr.sh defines _agr_record_session function
test_header "agr.sh defines _agr_record_session function"
if test_agr_sh 'type _agr_record_session 2>/dev/null | grep -q function'; then
    pass "_agr_record_session function is defined"
else
    fail "_agr_record_session function not defined"
fi

# Test: agr.sh defines _agr_setup_wrappers function
test_header "agr.sh defines _agr_setup_wrappers function"
if test_agr_sh 'type _agr_setup_wrappers 2>/dev/null | grep -q function'; then
    pass "_agr_setup_wrappers function is defined"
else
    fail "_agr_setup_wrappers function not defined"
fi

# Test: agr.sh exports _AGR_LOADED marker
test_header "agr.sh sets _AGR_LOADED marker"
if test_agr_sh '[ -n "$_AGR_LOADED" ]'; then
    pass "_AGR_LOADED marker is set"
else
    fail "_AGR_LOADED marker not set"
fi

# Test: Wrapper function created for configured agent
test_header "Wrapper function created for configured agent"
# Add a test agent first
$AGR agents add test-wrapper-agent
# Wrappers are now self-contained with _AGR_WRAPPER variable marker
if test_agr_sh 'declare -f test-wrapper-agent 2>/dev/null | grep -q "_AGR_WRAPPER"'; then
    pass "Wrapper function created for test-wrapper-agent"
else
    # Might need to re-setup wrappers
    if test_agr_sh '_agr_setup_wrappers && declare -f test-wrapper-agent 2>/dev/null | grep -q "_AGR_WRAPPER"'; then
        pass "Wrapper function created for test-wrapper-agent (after setup)"
    else
        fail "Wrapper function not created for test-wrapper-agent"
    fi
fi

# Test: Default agents have wrappers in fresh shell (conditional - agents may not be installed)
test_header "Default agents have wrappers in fresh shell"
WRAPPER_AGENT_FOUND=""
for AGENT in claude codex gemini; do
    if test_agr_sh "declare -f $AGENT 2>/dev/null | grep -q '_AGR_WRAPPER'"; then
        pass "Wrapper created for default agent: $AGENT"
        # Remember the first agent with a wrapper for later tests
        if [ -z "$WRAPPER_AGENT_FOUND" ]; then
            WRAPPER_AGENT_FOUND="$AGENT"
        fi
    else
        # Only skip if the agent CLI is not installed
        # If CLI IS installed, wrapper MUST be created - this is a real failure
        if agent_installed "$AGENT"; then
            fail "Wrapper NOT created for $AGENT (but CLI is installed!)"
        else
            skip "Wrapper not created for $AGENT (CLI not installed - OK in CI)"
        fi
    fi
done

# Test: Wrapper function structure is self-contained (uses first available agent)
test_header "Wrapper function is self-contained (survives shell snapshots)"
if [ -n "$WRAPPER_AGENT_FOUND" ]; then
    # The wrapper should contain all logic inline, not call external helper functions
    WRAPPER_DEF=$(test_agr_sh "declare -f $WRAPPER_AGENT_FOUND 2>/dev/null")
    # Check for key components that make wrapper self-contained
    if echo "$WRAPPER_DEF" | /usr/bin/grep -q "ASCIINEMA_REC" && \
       echo "$WRAPPER_DEF" | /usr/bin/grep -q "agr record" && \
       echo "$WRAPPER_DEF" | /usr/bin/grep -q "agr agents is-wrapped"; then
        pass "Wrapper function contains all required self-contained logic"
    else
        fail "Wrapper function missing self-contained components"
        echo "Wrapper definition:"
        echo "$WRAPPER_DEF"
    fi

    # Test: Wrapper does NOT call _agr_record_session (old pattern)
    test_header "Wrapper uses new self-contained pattern"
    if ! echo "$WRAPPER_DEF" | /usr/bin/grep -q "_agr_record_session"; then
        pass "Wrapper does not depend on external _agr_record_session function"
    else
        fail "Wrapper still uses old _agr_record_session pattern"
    fi
else
    skip "No default agent wrappers found - cannot test wrapper structure (OK in CI without agents)"
    test_header "Wrapper uses new self-contained pattern"
    skip "No default agent wrappers found - cannot test wrapper pattern (OK in CI without agents)"
fi

# Test: Wrapper invokes agr record (with mock agent)
test_header "Wrapper invokes agr record"
# Create a simple mock agent that just echoes
MOCK_AGENT_DIR="$TEST_DIR/bin"
mkdir -p "$MOCK_AGENT_DIR"
cat > "$MOCK_AGENT_DIR/mock-agent" << 'EOF'
#!/bin/bash
echo "Mock agent executed with args: $@"
EOF
chmod +x "$MOCK_AGENT_DIR/mock-agent"

# Add mock-agent to config
$AGR agents add mock-agent

# Test that invoking wrapper creates a recording
BEFORE_COUNT=$(ls "$HOME/recorded_agent_sessions/mock-agent/"*.cast 2>/dev/null | wc -l | tr -d ' ')

# Get the path to asciinema for the subshell
ASCIINEMA_PATH=$(command -v asciinema)
ASCIINEMA_DIR=$(dirname "$ASCIINEMA_PATH")
export ASCIINEMA_DIR

# Capture original PATH for the test subshell
ORIG_PATH="$PATH"

(
    # Preserve access to asciinema by including its directory in PATH
    export PATH="$MOCK_AGENT_DIR:$PROJECT_DIR/target/release:$ASCIINEMA_DIR:$ORIG_PATH"
    export HOME="$TEST_DIR"
    # Unset ASCIINEMA_REC so wrapper doesn't think we're already recording
    unset ASCIINEMA_REC
    # Run directly without eating stderr so we can debug if needed
    bash -c "source '$HOME/.config/agr/agr.sh' && mock-agent test-arg" </dev/null
)
AFTER_COUNT=$(ls "$HOME/recorded_agent_sessions/mock-agent/"*.cast 2>/dev/null | wc -l | tr -d ' ')
if [ "$AFTER_COUNT" -gt "$BEFORE_COUNT" ]; then
    pass "Wrapper invoked agr record and created recording"
else
    fail "Wrapper did not create recording (before=$BEFORE_COUNT, after=$AFTER_COUNT)"
fi

# Test: Wrapper skips when ASCIINEMA_REC is set
test_header "Wrapper skips when ASCIINEMA_REC is set"
BEFORE_COUNT=$(ls "$HOME/recorded_agent_sessions/mock-agent/"*.cast 2>/dev/null | wc -l | tr -d ' ')
(
    export PATH="$MOCK_AGENT_DIR:$PROJECT_DIR/target/release:$ASCIINEMA_DIR:$PATH"
    export HOME="$TEST_DIR"
    export ASCIINEMA_REC="/tmp/fake-recording.cast"
    bash -c "source '$HOME/.config/agr/agr.sh' && mock-agent skip-test" </dev/null 2>/dev/null
)
AFTER_COUNT=$(ls "$HOME/recorded_agent_sessions/mock-agent/"*.cast 2>/dev/null | wc -l | tr -d ' ')
if [ "$AFTER_COUNT" -eq "$BEFORE_COUNT" ]; then
    pass "Wrapper skipped recording when ASCIINEMA_REC is set"
else
    fail "Wrapper recorded even when ASCIINEMA_REC was set"
fi

# Test: Wrapper respects no_wrap list
test_header "Wrapper respects no_wrap list"
# Add mock-agent to no-wrap list
$AGR agents no-wrap add mock-agent
BEFORE_COUNT=$(ls "$HOME/recorded_agent_sessions/mock-agent/"*.cast 2>/dev/null | wc -l | tr -d ' ')
(
    export PATH="$MOCK_AGENT_DIR:$PROJECT_DIR/target/release:$ASCIINEMA_DIR:$PATH"
    export HOME="$TEST_DIR"
    bash -c "source '$HOME/.config/agr/agr.sh' && mock-agent nowrap-test" </dev/null 2>/dev/null
)
AFTER_COUNT=$(ls "$HOME/recorded_agent_sessions/mock-agent/"*.cast 2>/dev/null | wc -l | tr -d ' ')
if [ "$AFTER_COUNT" -eq "$BEFORE_COUNT" ]; then
    pass "Wrapper respects no_wrap list (no recording created)"
else
    fail "Wrapper ignored no_wrap list and created recording"
fi
# Clean up: remove from no-wrap
$AGR agents no-wrap remove mock-agent

# Test: Default agents are wrapped when no config
test_header "Default agents are wrapped when no config"
# Test that default agent names result in wrapper functions
# Note: we can't easily test this in isolation without removing config
# Instead, check that the agr.sh script has fallback logic
if /usr/bin/grep -q 'agents="claude codex gemini"' "$HOME/.config/agr/agr.sh"; then
    pass "agr.sh has default agent fallback"
else
    fail "agr.sh missing default agent fallback"
fi

# Test: Invalid agent names are rejected
test_header "Invalid agent names are rejected in wrapper setup"
# The wrapper should not create functions for agents with special characters
# This is tested implicitly by the regex check in _agr_setup_wrappers
if /usr/bin/grep -q '\[\[ ! "\$agent" =~' "$HOME/.config/agr/agr.sh" || \
   /usr/bin/grep -q 'alphanumeric.*dash.*underscore' "$HOME/.config/agr/agr.sh"; then
    pass "agr.sh validates agent names"
else
    # Check for the actual regex pattern
    if /usr/bin/grep -q '\^.*\-.*\+\$' "$HOME/.config/agr/agr.sh"; then
        pass "agr.sh validates agent names (regex found)"
    else
        fail "agr.sh missing agent name validation"
    fi
fi

# Clean up test agents
$AGR agents remove test-wrapper-agent 2>/dev/null || true
$AGR agents remove mock-agent 2>/dev/null || true

# Print summary when running standalone
if [ -z "$_AGR_E2E_MAIN_RUNNER" ]; then
    print_summary
    exit $?
fi
