#!/usr/bin/env bash
set -e

echo "=== Agent Session Recorder (AGR) Uninstaller ==="
echo

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
            if grep -q ">>> AGR (Agent Session Recorder) >>>" "$rc" 2>/dev/null; then
                sed -i.bak '/# >>> AGR (Agent Session Recorder) >>>/,/# <<< AGR (Agent Session Recorder) <<</d' "$rc"
                rm -f "$rc.bak"
                echo "  Removed shell integration from: $rc"
            fi
        fi
    done
fi

# Remove binary
echo
echo "Removing binary..."
if command -v cargo &>/dev/null; then
    cargo uninstall agr 2>/dev/null && echo "Removed agr via cargo" || echo "agr not installed via cargo"
else
    echo "cargo not found, skipping binary removal"
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
