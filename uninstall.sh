#!/usr/bin/env bash
set -e

echo "=== Agent Session Recorder (AGR) Uninstaller ==="
echo

INSTALL_DIR="$HOME/.local/bin"

# Remove skills (before binary removal so agr CLI can run)
echo "Removing skills..."
if command -v agr &>/dev/null; then
    agr skills uninstall
else
    # Fallback: manually remove skill files if agr is not available
    echo "agr not found in PATH, removing skills manually..."
    for dir in "$HOME/.claude/commands" "$HOME/.codex/commands" "$HOME/.gemini/commands"; do
        for skill in "agr-analyze.md" "agr-review.md"; do
            if [ -f "$dir/$skill" ] || [ -L "$dir/$skill" ]; then
                rm "$dir/$skill"
                echo "  Removed: $dir/$skill"
            fi
        done
    done
fi

# Remove shell integration (before binary removal so agr CLI can run)
echo
echo "Removing shell integration..."
if command -v agr &>/dev/null; then
    agr shell uninstall
else
    # Fallback: manually remove shell integration if agr is not available
    echo "agr not found in PATH, removing shell integration manually..."
    for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
        if [ -f "$rc" ]; then
            # Check if AGR markers are present
            if grep -q ">>> AGR (Agent Session Recorder) >>>" "$rc" 2>/dev/null; then
                # Remove the marked section using sed
                sed -i.bak '/# >>> AGR (Agent Session Recorder) >>>/,/# <<< AGR (Agent Session Recorder) <<</d' "$rc"
                rm -f "$rc.bak"
                echo "  Removed shell integration from: $rc"
            fi
        fi
    done
    # Also remove the shell script
    if [ -f "$HOME/.config/agr/agr.sh" ]; then
        rm "$HOME/.config/agr/agr.sh"
        echo "  Removed: $HOME/.config/agr/agr.sh"
    fi
fi

# Remove binary (after CLI cleanup so agr commands can run)
echo
echo "Removing binary..."
if [ -f "$INSTALL_DIR/agr" ]; then
    rm "$INSTALL_DIR/agr"
    echo "Removed binary: $INSTALL_DIR/agr"
else
    echo "Binary not found at: $INSTALL_DIR/agr"
fi

# Remove config directory (ask first)
CONFIG_DIR="$HOME/.config/agr"
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

echo
echo "=== Uninstallation Complete ==="
