# Agent Session Recorder - Shell Integration
# Source this file from your .zshrc or .bashrc

# Mark that AGR shell integration is loaded
export _AGR_LOADED=1

# Only set up if asciinema is available and we're not already recording
_agr_record_session() {
    local agent="$1"
    shift

    # Don't wrap if already in a recording session
    if [[ -n "${ASCIINEMA_REC:-}" ]]; then
        command "$agent" "$@"
        return
    fi

    # Don't wrap if asciinema isn't available
    if ! command -v asciinema &>/dev/null; then
        command "$agent" "$@"
        return
    fi

    # Don't wrap if agr isn't available
    if ! command -v agr &>/dev/null; then
        command "$agent" "$@"
        return
    fi

    # Check if this agent should be wrapped (respects no_wrap list and auto_wrap toggle)
    if ! agr agents is-wrapped "$agent" 2>/dev/null; then
        command "$agent" "$@"
        return
    fi

    # Record the session
    agr record "$agent" -- "$@"
}

# Generate wrapper functions from config
_agr_setup_wrappers() {
    local agents

    # Try to get agent list from agr
    if command -v agr &>/dev/null; then
        agents=$(agr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
    fi

    # Fallback to default agents if agr not available
    if [[ -z "$agents" ]]; then
        agents="claude codex gemini-cli"
    fi

    # Create wrapper for each agent
    for agent in $agents; do
        # Skip if a function already exists with a different definition
        if type "$agent" 2>/dev/null | grep -q "_agr_record_session"; then
            continue
        fi

        # Create the wrapper function
        eval "$agent() { _agr_record_session $agent \"\$@\"; }"
    done
}

# Initialize wrappers
_agr_setup_wrappers
