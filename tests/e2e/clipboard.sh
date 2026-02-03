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

# Test: agr copy command works
test_copy_command() {
    local output
    output=$("$AGR" copy "$TEST_CAST" 2>&1)
    if [[ "$output" == *"Copied"*"clipboard"* ]]; then
        pass "agr copy produces success message"
    else
        fail "agr copy did not produce expected message: $output"
    fi
}

# Test: clipboard actually contains file reference (macOS) or content (Linux)
test_clipboard_content() {
    "$AGR" copy "$TEST_CAST" 2>/dev/null

    if [[ "$OSTYPE" == "darwin"* ]]; then
        # On macOS, check clipboard has file URL type
        local clip_info
        clip_info=$(osascript -e 'clipboard info' 2>/dev/null)
        if [[ "$clip_info" == *"furl"* ]] || [[ "$clip_info" == *"public.file-url"* ]]; then
            pass "macOS clipboard contains file reference"
        else
            fail "macOS clipboard does not contain file reference: $clip_info"
        fi
    else
        # On Linux, check clipboard has content (xclip)
        if command -v xclip &>/dev/null; then
            local content
            content=$(xclip -selection clipboard -o 2>/dev/null || true)
            if [[ -n "$content" ]]; then
                pass "Linux clipboard contains content"
            else
                fail "Linux clipboard is empty after copy"
            fi
        else
            pass "Linux clipboard test skipped (xclip not available for verification)"
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
    if [[ "$output" == *"not found"* ]] || [[ "$output" == *"No such file"* ]]; then
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
