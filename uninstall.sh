#!/usr/bin/env bash
set -e

echo "=== Agent Session Recorder Uninstaller ==="
echo

INSTALL_DIR="$HOME/.local/bin"

# Remove skills (before binary removal so asr CLI can run)
echo "Removing skills..."
if command -v asr &>/dev/null; then
    asr skills uninstall
else
    # Fallback: manually remove skill files if asr is not available
    echo "asr not found in PATH, removing skills manually..."
    for dir in "$HOME/.claude/commands" "$HOME/.codex/commands" "$HOME/.gemini/commands"; do
        for skill in "asr-analyze.md" "asr-review.md"; do
            if [ -f "$dir/$skill" ] || [ -L "$dir/$skill" ]; then
                rm "$dir/$skill"
                echo "  Removed: $dir/$skill"
            fi
        done
    done
fi

# Remove shell integration (before binary removal so asr CLI can run)
echo
echo "Removing shell integration..."
if command -v asr &>/dev/null; then
    asr shell uninstall
else
    # Fallback: manually remove shell integration if asr is not available
    echo "asr not found in PATH, removing shell integration manually..."
    for rc in "$HOME/.zshrc" "$HOME/.bashrc"; do
        if [ -f "$rc" ]; then
            # Check if ASR markers are present
            if grep -q ">>> ASR (Agent Session Recorder) >>>" "$rc" 2>/dev/null; then
                # Remove the marked section using sed
                sed -i.bak '/# >>> ASR (Agent Session Recorder) >>>/,/# <<< ASR (Agent Session Recorder) <<</d' "$rc"
                rm -f "$rc.bak"
                echo "  Removed shell integration from: $rc"
            fi
        fi
    done
    # Also remove the shell script
    if [ -f "$HOME/.config/asr/asr.sh" ]; then
        rm "$HOME/.config/asr/asr.sh"
        echo "  Removed: $HOME/.config/asr/asr.sh"
    fi
fi

# Remove binary (after CLI cleanup so asr commands can run)
echo
echo "Removing binary..."
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

echo
echo "=== Uninstallation Complete ==="
