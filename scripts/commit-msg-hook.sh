#!/bin/bash
# Git commit-msg hook: enforce scope matches src/ module structure
# Install: cp scripts/commit-msg-hook.sh .git/hooks/commit-msg && chmod +x .git/hooks/commit-msg

COMMIT_MSG_FILE="$1"
COMMIT_MSG=$(cat "$COMMIT_MSG_FILE")

# Valid scopes from src/ structure
VALID_SCOPES=(
    # Directories
    "asciicast"
    "commands"
    "player"
    "terminal"
    "tui"
    # Single-file modules
    "analyzer"
    "branding"
    "cli"
    "config"
    "recording"
    "shell"
    "storage"
    # Meta scopes (not in src/ but valid for certain commits)
    "deps"      # dependency updates
    "ci"        # CI/CD changes
    "docs"      # documentation
    "tests"     # test infrastructure
    "release"   # release automation
)

# Extract scope from conventional commit: type(scope): message
if [[ "$COMMIT_MSG" =~ ^[a-z]+\(([a-z0-9-]+)\): ]]; then
    SCOPE="${BASH_REMATCH[1]}"

    # Check if scope is valid
    VALID=false
    for valid_scope in "${VALID_SCOPES[@]}"; do
        if [[ "$SCOPE" == "$valid_scope" ]]; then
            VALID=true
            break
        fi
    done

    if [[ "$VALID" == "false" ]]; then
        echo "ERROR: Invalid scope '($SCOPE)' in commit message."
        echo ""
        echo "Valid scopes (from src/ modules):"
        echo "  asciicast, commands, player, terminal, tui"
        echo "  analyzer, branding, cli, config, recording, shell, storage"
        echo ""
        echo "Meta scopes:"
        echo "  deps, ci, docs, tests, release"
        echo ""
        echo "Your commit: $COMMIT_MSG"
        exit 1
    fi
fi

# Allow commits without scope (e.g., "feat: something")
exit 0
