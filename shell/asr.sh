# Agent Session Recorder - Shell Integration
# Source this file from your .zshrc or .bashrc

# Only set up if asciinema is available and we're not already recording
_asr_record_session() {
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

    # Don't wrap if asr isn't available
    if ! command -v asr &>/dev/null; then
        command "$agent" "$@"
        return
    fi

    # Record the session
    asr record "$agent" -- "$@"
}

# Generate wrapper functions from config
_asr_setup_wrappers() {
    local agents

    # Try to get agent list from asr
    if command -v asr &>/dev/null; then
        agents=$(asr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
    fi

    # Fallback to default agents if asr not available
    if [[ -z "$agents" ]]; then
        agents="claude codex gemini-cli"
    fi

    # Create wrapper for each agent
    for agent in $agents; do
        # Skip if a function already exists with a different definition
        if type "$agent" 2>/dev/null | grep -q "_asr_record_session"; then
            continue
        fi

        # Create the wrapper function
        eval "$agent() { _asr_record_session $agent \"\$@\"; }"
    done
}

# Initialize wrappers
_asr_setup_wrappers
