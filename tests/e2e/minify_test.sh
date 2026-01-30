#!/usr/bin/env bash
# E2E tests for shell minification
set -e

# Test minified zsh sources without error
test_zsh_sources() {
    local output=$(cargo run --quiet -- completions --shell-init zsh 2>/dev/null)
    if command -v zsh &>/dev/null; then
        echo "$output" | zsh -n  # Syntax check
        echo "PASS: zsh syntax valid"
    else
        echo "SKIP: zsh not available"
    fi
}

# Test minified bash sources without error
test_bash_sources() {
    local output=$(cargo run --quiet -- completions --shell-init bash 2>/dev/null)
    echo "$output" | bash -n  # Syntax check
    echo "PASS: bash syntax valid"
}

# Test completion function defined in zsh
test_zsh_completion_defined() {
    if command -v zsh &>/dev/null; then
        local output=$(cargo run --quiet -- completions --shell-init zsh 2>/dev/null)
        if echo "$output" | grep -q "_agr_complete"; then
            echo "PASS: _agr_complete function present in zsh"
        else
            echo "FAIL: _agr_complete function missing"
            exit 1
        fi
    else
        echo "SKIP: zsh not available"
    fi
}

# Test completion function defined in bash
test_bash_completion_defined() {
    local output=$(cargo run --quiet -- completions --shell-init bash 2>/dev/null)
    if echo "$output" | grep -q "_agr_complete"; then
        echo "PASS: _agr_complete function present in bash"
    else
        echo "FAIL: _agr_complete function missing"
        exit 1
    fi
}

# Test debug flag produces readable output
test_debug_output() {
    local compressed=$(cargo run --quiet -- completions --shell-init zsh 2>/dev/null | wc -l)
    local debug=$(cargo run --quiet -- completions --shell-init zsh --debug 2>/dev/null | wc -l)

    if [ "$debug" -gt "$compressed" ]; then
        echo "PASS: debug output ($debug lines) > compressed ($compressed lines)"
    else
        echo "FAIL: debug should produce more lines than compressed"
        exit 1
    fi
}

# Test line count targets
test_line_counts() {
    local zsh_lines=$(cargo run --quiet -- completions --shell-init zsh 2>/dev/null | wc -l | tr -d ' ')
    local bash_lines=$(cargo run --quiet -- completions --shell-init bash 2>/dev/null | wc -l | tr -d ' ')

    echo "Zsh: $zsh_lines lines"
    echo "Bash: $bash_lines lines"

    if [ "$zsh_lines" -le 15 ]; then
        echo "PASS: zsh within 15 line target"
    else
        echo "FAIL: zsh exceeds 15 lines"
        exit 1
    fi

    if [ "$bash_lines" -le 15 ]; then
        echo "PASS: bash within 15 line target"
    else
        echo "FAIL: bash exceeds 15 lines"
        exit 1
    fi
}

echo "=== E2E Minification Tests ==="
test_zsh_sources
test_bash_sources
test_zsh_completion_defined
test_bash_completion_defined
test_debug_output
test_line_counts
echo "=== All E2E tests passed ==="
