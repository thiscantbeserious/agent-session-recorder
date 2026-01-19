#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

echo "=== Agent Session Recorder Installer ==="
echo

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Darwin) OS_NAME="macOS" ;;
    Linux)  OS_NAME="Linux" ;;
    *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac
echo "Detected OS: $OS_NAME"

# Check for asciinema
if ! command -v asciinema &>/dev/null; then
    echo
    echo "asciinema not found. Installing..."
    case "$OS" in
        Darwin)
            if command -v brew &>/dev/null; then
                brew install asciinema
            else
                echo "Please install Homebrew or asciinema manually."
                exit 1
            fi
            ;;
        Linux)
            if command -v apt-get &>/dev/null; then
                sudo apt-get update && sudo apt-get install -y asciinema
            elif command -v dnf &>/dev/null; then
                sudo dnf install -y asciinema
            elif command -v pacman &>/dev/null; then
                sudo pacman -S asciinema
            else
                echo "Please install asciinema manually."
                exit 1
            fi
            ;;
    esac
fi
echo "asciinema: $(command -v asciinema)"

# Build binary
echo
echo "Building binary..."

# Try native build first (if cargo available)
if command -v cargo &>/dev/null; then
    echo "Using native Rust build..."
    cargo build --release
    mkdir -p dist
    cp target/release/asr dist/asr
# Fallback to Docker for Linux binary
elif command -v docker &>/dev/null; then
    echo "Using Docker build (produces Linux binary)..."
    ./build.sh
else
    echo "Error: Neither cargo nor docker found. Please install one of them."
    exit 1
fi

# Install binary
INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"
cp dist/asr "$INSTALL_DIR/asr"
chmod +x "$INSTALL_DIR/asr"
echo
echo "Installed binary to: $INSTALL_DIR/asr"

# Create session directory
SESSION_DIR="$HOME/recorded_agent_sessions"
mkdir -p "$SESSION_DIR"
echo "Created session directory: $SESSION_DIR"

# Create config directory
CONFIG_DIR="$HOME/.config/asr"
mkdir -p "$CONFIG_DIR"
echo "Created config directory: $CONFIG_DIR"

# Setup shell integration
SHELL_SCRIPT="$(pwd)/shell/asr.sh"
SHELL_RC=""

if [ -n "$ZSH_VERSION" ] || [ -f "$HOME/.zshrc" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -n "$BASH_VERSION" ] || [ -f "$HOME/.bashrc" ]; then
    SHELL_RC="$HOME/.bashrc"
fi

if [ -n "$SHELL_RC" ]; then
    SOURCE_LINE="[ -f \"$SHELL_SCRIPT\" ] && source \"$SHELL_SCRIPT\""
    if ! grep -q "asr.sh" "$SHELL_RC" 2>/dev/null; then
        echo >> "$SHELL_RC"
        echo "# Agent Session Recorder" >> "$SHELL_RC"
        echo "$SOURCE_LINE" >> "$SHELL_RC"
        echo "Added shell integration to: $SHELL_RC"
    else
        echo "Shell integration already present in: $SHELL_RC"
    fi
fi

# Setup agent skills (symlinks)
setup_skill() {
    local skill_dir="$1"
    local skill_name="$2"
    local skill_source="$(pwd)/agents/$skill_name"

    if [ -f "$skill_source" ]; then
        mkdir -p "$skill_dir"
        ln -sf "$skill_source" "$skill_dir/$skill_name"
        echo "  Linked: $skill_dir/$skill_name"
    fi
}

echo
echo "Setting up agent skills..."
setup_skill "$HOME/.claude/commands" "asr-analyze.md"
setup_skill "$HOME/.claude/commands" "asr-review.md"
setup_skill "$HOME/.codex/commands" "asr-analyze.md"
setup_skill "$HOME/.codex/commands" "asr-review.md"
setup_skill "$HOME/.gemini/commands" "asr-analyze.md"
setup_skill "$HOME/.gemini/commands" "asr-review.md"

# Verify installation
echo
echo "=== Installation Complete ==="
echo

# Check if asr is in PATH
if command -v asr &>/dev/null; then
    echo "✓ asr is available in PATH"
    asr --version
else
    echo "Note: Add $INSTALL_DIR to your PATH:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
fi

echo
echo "Next steps:"
echo "  1. Restart your shell or run: source $SHELL_RC"
echo "  2. Test with: asr --help"
echo "  3. Record a session: asr record claude"
