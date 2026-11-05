#!/bin/bash
# Real-TTY tests for the once-per-version compress offer (expect gives agr a PTY).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

if [[ -z "$_AGR_E2E_MAIN_RUNNER" ]]; then
    check_prerequisites || exit 1
    section "AGR Compress Offer Tests"
fi

if ! command -v expect &>/dev/null; then
    skip "expect not installed, skipping compress offer tests"
    return 0 2>/dev/null || exit 0
fi

section "Compress Offer (real TTY)"

OFFER_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$SCRIPT_DIR/../../Cargo.toml" | head -1)
OFFER_CAST='{"version":3,"term":{"cols":80,"rows":24}}
[0.5,"o","hello hello hello hello"]'

# Fresh isolated HOME with 2 plain sessions and no state.toml
offer_setup() {
    OFFER_HOME="$TEST_DIR/offer_home_$1"
    mkdir -p "$OFFER_HOME/.config/agr" "$OFFER_HOME/sessions/claude"
    printf '[storage]\ndirectory = "%s"\n' "$OFFER_HOME/sessions" > "$OFFER_HOME/.config/agr/config.toml"
    printf '%s\n' "$OFFER_CAST" > "$OFFER_HOME/sessions/claude/a.cast"
    printf '%s\n' "$OFFER_CAST" > "$OFFER_HOME/sessions/claude/b.cast"
}

# offer_run <subcommand> <answer or -> <after-script>: prompt expected unless answer is "-"
offer_run() {
    local sub="$1" answer="$2" after="${3:-}"
    local expect_prompt='expect { -re {Compress 2 session\(s\)\? \[y/N\]} {} timeout { exit 3 } }
        send "'"$answer"'\r"'
    [[ "$answer" == "-" ]] && expect_prompt='expect { -re {Compress 2 session} { exit 4 } timeout {} }'
    expect -c "
        log_user 0
        set timeout 5
        spawn env HOME=$OFFER_HOME XDG_CONFIG_HOME=$OFFER_HOME/.config TERM=xterm $AGR $sub
        $expect_prompt
        $after
        expect { eof {} timeout { exit 124 } }
        catch wait result
        exit [lindex \$result 3]
    " >/dev/null 2>&1
}

offer_state_has_version() {
    grep -q "$OFFER_VERSION" "$OFFER_HOME/.config/agr/state.toml" 2>/dev/null
}

test_header "offer on 'agr status', answer n keeps files and remembers the version"
offer_setup 1
offer_run status n && RC=0 || RC=$?
if [[ $RC -eq 0 && -f "$OFFER_HOME/sessions/claude/a.cast" && -f "$OFFER_HOME/sessions/claude/b.cast" \
      && ! -e "$OFFER_HOME/sessions/claude/a.cast.gz" ]] && offer_state_has_version; then
    pass "decline leaves plain files and saves the version"
else
    fail "decline flow wrong (exit=$RC)"
fi

test_header "second 'agr status' of the same version does not prompt"
offer_run status - "" && RC=0 || RC=$?
if [[ $RC -eq 0 ]]; then
    pass "no prompt on the second run"
else
    fail "prompt shown again or run failed (exit=$RC)"
fi

test_header "offer on 'agr status', answer y compresses and remembers the version"
offer_setup 2
offer_run status y && RC=0 || RC=$?
OK=1
for n in a b; do
    f="$OFFER_HOME/sessions/claude/$n.cast"
    [[ -e "$f" || ! -f "$f.gz" ]] && OK=0
    [[ "$(gzip -dc "$f.gz" 2>/dev/null)" == "$OFFER_CAST" ]] || OK=0
done
if [[ $RC -eq 0 && $OK -eq 1 ]] && offer_state_has_version; then
    pass "accept compresses both, content intact, version saved"
else
    fail "accept flow wrong (exit=$RC)"
fi

test_header "offer on 'agr list' comes before the TUI, then q quits cleanly"
offer_setup 3
offer_run list n 'sleep 1
        send "q"' && RC=0 || RC=$?
if [[ $RC -eq 0 ]] && offer_state_has_version; then
    pass "list offer then TUI quits with exit 0"
else
    fail "list offer flow wrong (exit=$RC)"
fi
