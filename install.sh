#!/usr/bin/env bash
set -e

cd "$(dirname "$0")"

echo "=== Agent Session Recorder (AGR) Installer ==="
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

# Install binary
echo
echo "Installing binary..."

if command -v cargo &>/dev/null; then
    echo "Using cargo install..."
    cargo install --path . --force
    INSTALL_DIR="$HOME/.cargo/bin"
else
    echo "Error: cargo not found. Please install Rust: https://rustup.rs"
    exit 1
fi

echo
echo "Installed binary to: $INSTALL_DIR/agr"

# Create session directory
SESSION_DIR="$HOME/recorded_agent_sessions"
mkdir -p "$SESSION_DIR"
echo "Created session directory: $SESSION_DIR"

# Create config directory
CONFIG_DIR="$HOME/.config/agr"
mkdir -p "$CONFIG_DIR"
echo "Created config directory: $CONFIG_DIR"

# Setup shell integration using agr shell install
echo
echo "Setting up shell integration..."
"$INSTALL_DIR/agr" shell install

# Verify installation
echo
echo "=== Installation Complete ==="
echo

# Check if agr is in PATH
if command -v agr &>/dev/null; then
    echo "✓ agr is available in PATH"
    agr --version
else
    echo "Note: Add $INSTALL_DIR to your PATH:"
    echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
fi

echo
echo "Next steps:"
echo "  1. Restart your shell to activate shell integration"
echo "  2. Test with: agr --help"
echo "  3. Record a session: agr record claude"
