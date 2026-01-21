# Agent Session Recorder - Shell Integration
# Source this file from your .zshrc or .bashrc

# Mark that AGR shell integration is loaded
export _AGR_LOADED=1

# Source completions based on shell type
_agr_setup_completions() {
    local completion_dir

    if [[ -n "$ZSH_VERSION" ]]; then
        # Zsh: Add completion directory to fpath if it exists
        completion_dir="${HOME}/.zsh/completions"
        if [[ -d "$completion_dir" ]]; then
            # Add to fpath if not already there
            if [[ ! " ${fpath[*]} " =~ " ${completion_dir} " ]]; then
                fpath=("$completion_dir" $fpath)
            fi
            # Reinitialize completions if compinit is available
            if command -v compinit &>/dev/null && [[ -f "$completion_dir/_agr" ]]; then
                autoload -Uz compinit
                compinit -i 2>/dev/null
            fi
        fi
    elif [[ -n "$BASH_VERSION" ]]; then
        # Bash: Source the completion file if it exists
        completion_dir="${HOME}/.local/share/bash-completion/completions"
        if [[ -f "$completion_dir/agr" ]]; then
            source "$completion_dir/agr"
        fi
    fi
}

# Initialize completions
_agr_setup_completions

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
# Each wrapper is self-contained to survive shell snapshots (e.g., Claude Code's shell-snapshots)
_agr_setup_wrappers() {
    local agents agent

    # Try to get agent list from agr
    if command -v agr &>/dev/null; then
        agents=$(agr agents list 2>/dev/null | grep -v "^Configured" | grep -v "^No agents" | sed 's/^  //')
    fi

    # Fallback to default agents if agr not available
    if [[ -z "$agents" ]]; then
        agents="claude codex gemini"
    fi

    # Create wrapper for each agent using while read to avoid word-splitting issues
    while IFS= read -r agent; do
        # Skip empty agent names
        [[ -z "$agent" ]] && continue

        # Validate agent name (alphanumeric, dash, underscore only)
        [[ ! "$agent" =~ ^[a-zA-Z0-9_-]+$ ]] && continue

        # Skip if a self-contained AGR wrapper already exists for this agent
        if declare -f "$agent" 2>/dev/null | grep -q "_AGR_WRAPPER"; then
            continue
        fi

        # Create self-contained wrapper function
        # The wrapper is self-contained so it survives shell snapshots that might not include helper functions
        eval "${agent}() {
            local _AGR_WRAPPER=1
            # Don't wrap if already in a recording session
            if [[ -n \"\${ASCIINEMA_REC:-}\" ]]; then
                command ${agent} \"\$@\"
                return
            fi
            # Don't wrap if asciinema or agr aren't available
            if ! command -v asciinema &>/dev/null || ! command -v agr &>/dev/null; then
                command ${agent} \"\$@\"
                return
            fi
            # Check if this agent should be wrapped
            if ! agr agents is-wrapped \"${agent}\" 2>/dev/null; then
                command ${agent} \"\$@\"
                return
            fi
            # Record the session
            agr record \"${agent}\" -- \"\$@\"
        }"
    done <<< "$agents"
}

# Initialize wrappers
_agr_setup_wrappers
