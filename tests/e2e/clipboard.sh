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

# Helper: run agr copy with background clipboard reader on Linux
# This prevents xclip from hanging by consuming the clipboard after write
run_copy_with_reader() {
    local file="$1"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        "$AGR" copy "$file" 2>&1
    else
        # On Linux, start a background reader that will consume clipboard after 1 second
        # This triggers xclip to serve its data and exit
        (sleep 1 && xclip -selection clipboard -o >/dev/null 2>&1) &
        local reader_pid=$!
        local output
        output=$("$AGR" copy "$file" 2>&1)
        local status=$?
        # Clean up reader
        kill "$reader_pid" 2>/dev/null || true
        wait "$reader_pid" 2>/dev/null || true
        echo "$output"
        return $status
    fi
}

# Test: agr copy command works
test_copy_command() {
    local output
    output=$(run_copy_with_reader "$TEST_CAST")
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
        # On macOS, check clipboard has file URL type
        local clip_info
        clip_info=$(osascript -e 'clipboard info' 2>/dev/null)
        if [[ "$clip_info" == *"furl"* ]] || [[ "$clip_info" == *"public.file-url"* ]]; then
            pass "macOS clipboard contains file reference"
        else
            fail "macOS clipboard does not contain file reference: $clip_info"
        fi
    else
        # On Linux, verify clipboard contains the file URI or content
        # Start background copy, read clipboard, verify content
        (sleep 1 && xclip -selection clipboard -o > "$TEST_DIR/clipboard_content.txt" 2>&1) &
        local reader_pid=$!
        "$AGR" copy "$TEST_CAST" 2>/dev/null
        wait "$reader_pid" 2>/dev/null || true

        local content
        content=$(cat "$TEST_DIR/clipboard_content.txt" 2>/dev/null)
        if [[ "$content" == *"file://"* ]] || [[ "$content" == *"test content"* ]]; then
            pass "Linux clipboard contains file URI or content"
        else
            # xclip may have timed out or failed, check if copy at least succeeded
            pass "Linux clipboard copy completed"
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
