#!/usr/bin/env bash
set -e

echo "=== Agent Session Recorder Uninstaller ==="
echo

# Remove binary
INSTALL_DIR="$HOME/.local/bin"
if [ -f "$INSTALL_DIR/asr" ]; then
    rm "$INSTALL_DIR/asr"
    echo "Removed binary: $INSTALL_DIR/asr"
else
    echo "Binary not found at: $INSTALL_DIR/asr"
fi

# Remove config directory (ask first)
CONFIG_DIR="$HOME/.config/asr"
if [ -d "$CONFIG_DIR" ]; then
    read -p "Remove config directory ($CONFIG_DIR)? [y/N]: " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$CONFIG_DIR"
        echo "Removed config directory"
    else
        echo "Kept config directory"
    fi
fi

# Remove session directory (ask first)
SESSION_DIR="$HOME/recorded_agent_sessions"
if [ -d "$SESSION_DIR" ]; then
    read -p "Remove session recordings ($SESSION_DIR)? [y/N]: " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$SESSION_DIR"
        echo "Removed session directory"
    else
        echo "Kept session directory"
    fi
fi

# Remove skill symlinks
remove_skill() {
    local skill_path="$1"
    if [ -L "$skill_path" ]; then
        rm "$skill_path"
        echo "Removed skill: $skill_path"
    fi
}

echo
echo "Removing agent skills..."
remove_skill "$HOME/.claude/commands/asr-analyze.md"
remove_skill "$HOME/.claude/commands/asr-review.md"
remove_skill "$HOME/.codex/commands/asr-analyze.md"
remove_skill "$HOME/.codex/commands/asr-review.md"
remove_skill "$HOME/.gemini/commands/asr-analyze.md"
remove_skill "$HOME/.gemini/commands/asr-review.md"

# Note about shell integration
echo
echo "Note: Shell integration line in .zshrc/.bashrc was NOT removed."
echo "You can manually remove the 'Agent Session Recorder' section if desired."

echo
echo "=== Uninstallation Complete ==="
