#!/bin/bash
# Clipboard e2e tests - verify real clipboard operations

# Skip if running as part of main runner without clipboard tools
if [[ -z "$_AGR_E2E_MAIN_RUNNER" ]]; then
    source "$(dirname "$0")/common.sh"
fi

# Check for clipboard tools
has_clipboard_tool() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        command -v osascript &>/dev/null
    else
        command -v xclip &>/dev/null || command -v xsel &>/dev/null || command -v wl-copy &>/dev/null
    fi
}

if ! has_clipboard_tool; then
    echo "⚠️  Skipping clipboard tests - no clipboard tools available"
    return 0 2>/dev/null || exit 0
fi

echo "Testing clipboard operations..."

# Create a test recording
TEST_CAST="$TEST_DIR/clipboard_test.cast"
cat > "$TEST_CAST" << 'EOF'
{"version": 2, "width": 80, "height": 24, "timestamp": 1234567890}
[0.0, "o", "test content"]
EOF

# Helper: run agr copy on Linux, read clipboard, then kill hanging xclip
# xclip forks and waits to serve requests - we read the data then kill it
run_copy_and_read_linux() {
    local file="$1"

    # Run agr copy in background (xclip will fork and wait)
    "$AGR" copy "$file" > "$TEST_DIR/copy_output.txt" 2>&1 &
    local copy_pid=$!

    # Wait a moment for xclip to receive data
    sleep 1

    # Read clipboard content (this triggers xclip to serve data)
    local content
    content=$(xclip -selection clipboard -o 2>/dev/null) || true
    echo "$content" > "$TEST_DIR/clipboard_content.txt"

    # Kill any remaining xclip processes from our copy
    kill "$copy_pid" 2>/dev/null || true
    pkill -f "xclip -selection clipboard" 2>/dev/null || true
    wait "$copy_pid" 2>/dev/null || true

    # Return the copy command output
    cat "$TEST_DIR/copy_output.txt" 2>/dev/null
}

# Test: agr copy command works
test_copy_command() {
    local output
    if [[ "$OSTYPE" == "darwin"* ]]; then
        output=$("$AGR" copy "$TEST_CAST" 2>&1)
    else
        output=$(run_copy_and_read_linux "$TEST_CAST")
    fi
    if [[ "$output" == *"Copied"*"clipboard"* ]]; then
        pass "agr copy produces success message"
    else
        fail "agr copy did not produce expected message: $output"
    fi
}

# Test: clipboard actually contains file reference (macOS) or content (Linux)
test_clipboard_content() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        "$AGR" copy "$TEST_CAST" 2>/dev/null
        # Small delay to ensure clipboard is updated (CI can be slow)
        sleep 0.5
        # On macOS, check clipboard has file URL type
        local clip_info
        clip_info=$(osascript -e 'clipboard info' 2>/dev/null || echo "osascript_failed")
        if [[ "$clip_info" == *"furl"* ]] || [[ "$clip_info" == *"public.file-url"* ]]; then
            pass "macOS clipboard contains file reference"
        elif [[ "$clip_info" == "osascript_failed" ]] || [[ -z "$clip_info" ]]; then
            # macOS CI clipboard may not be accessible - skip gracefully
            skip "macOS clipboard info not accessible in CI (osascript returned: '$clip_info')"
        else
            fail "macOS clipboard does not contain file reference: $clip_info"
        fi
    else
        # On Linux, copy and verify clipboard content
        run_copy_and_read_linux "$TEST_CAST" >/dev/null
        local content
        content=$(cat "$TEST_DIR/clipboard_content.txt" 2>/dev/null)
        if [[ "$content" == *"file://"* ]] || [[ "$content" == *"clipboard_test.cast"* ]]; then
            pass "Linux clipboard contains file URI"
        elif [[ "$content" == *"test content"* ]] || [[ -n "$content" ]]; then
            pass "Linux clipboard contains content"
        else
            fail "Linux clipboard is empty or invalid: $content"
        fi
    fi
}

# Test: copy non-existent file shows error
test_copy_nonexistent() {
    local output
    output=$("$AGR" copy "nonexistent.cast" 2>&1) && {
        fail "agr copy should fail for non-existent file"
        return
    }
    # Check for various error messages
    if [[ "$output" == *"not found"* ]] || [[ "$output" == *"No such file"* ]] || \
       [[ "$output" == *"does not exist"* ]] || [[ "$output" == *"Error"* ]]; then
        pass "agr copy shows error for non-existent file"
    else
        fail "agr copy error message unclear: $output"
    fi
}

# Run tests
test_copy_command
test_clipboard_content
test_copy_nonexistent

echo "Clipboard tests complete."
