#!/bin/bash
# Sync tests, no GitHub involved: AGR_SYNC_TEST_BACKEND_DIR swaps gh for a directory fake.
# Tests: first push, status, second machine via key export/import, conflict, delete,
# auto push after a recording, no-TTY behaviour.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

if [[ -z "$_AGR_E2E_MAIN_RUNNER" ]]; then
    check_prerequisites || exit 1
    section "AGR Sync Tests"
    create_ci_config
fi

section "Sync (file backend)"

SYNC_ROOT="$TEST_DIR/sync"
SYNC_REMOTE="$SYNC_ROOT/remote"
mkdir -p "$SYNC_REMOTE"
HOME_A="$SYNC_ROOT/home_a"
HOME_B="$SYNC_ROOT/home_b"

# sync_setup <home>: isolated HOME with sync on and its own sessions dir
sync_setup() {
    mkdir -p "$1/.config/agr" "$1/sessions"
    printf '[storage]\ndirectory = "%s"\n[sync]\nbackend = "gist"\n[recording]\nfilename_template = "{directory}_{date}_{time:%%H%%M%%S%%f}"\n' "$1/sessions" > "$1/.config/agr/config.toml"
}

# sync_agr <home> <args...>: run agr as that machine
sync_agr() {
    local h="$1"; shift
    HOME="$h" XDG_CONFIG_HOME="$h/.config" NO_COLOR=1 AGR_SYNC_TEST_BACKEND_DIR="$SYNC_REMOTE" "$AGR" sync "$@" </dev/null
}

sync_session() {
    mkdir -p "$1/sessions/$2"
    printf '{"version":3,"term":{"cols":80,"rows":24}}\n[0.5,"o","%s"]\n' "$4" | gzip -n > "$1/sessions/$2/$3"
}

sync_setup "$HOME_A"
sync_setup "$HOME_B"
sync_session "$HOME_A" claude one.cast.gz "hello from one"

test_header "no TTY without --yes changes nothing"
OUT=$(sync_agr "$HOME_A" push 2>&1) || true
if echo "$OUT" | grep -q "No changes made" && [[ ! -f "$HOME_A/.config/agr/sync.key" ]] \
   && ! ls "$SYNC_REMOTE"/*.blob >/dev/null 2>&1; then
    pass "push without --yes aborted, no key, nothing uploaded"
else
    fail "push without --yes changed something: $OUT"
fi

test_header "dry run and status work without a TTY"
if sync_agr "$HOME_A" push --dry-run 2>&1 | grep -q "Would push 1 session" \
   && sync_agr "$HOME_A" status 2>&1 | grep -q "local only (push)"; then
    pass "dry run and status work"
else
    fail "dry run or status failed"
fi

test_header "first push shows the privacy notice and uploads ciphertext"
OUT=$(sync_agr "$HOME_A" push --yes 2>&1)
BLOB=$(ls "$SYNC_REMOTE"/*.blob 2>/dev/null | head -1)
if echo "$OUT" | grep -q "anyone with the link" && echo "$OUT" | grep -q "Pushed: 1" \
   && [[ -f "$HOME_A/.config/agr/sync.key" ]] && grep -q "sync_privacy_ack = true" "$HOME_A/.config/agr/state.toml" \
   && [[ -n "$BLOB" ]] && ! grep -aq "hello from one" "$BLOB"; then
    pass "privacy notice, key created, blob is ciphertext"
else
    fail "first push wrong: $OUT"
fi

test_header "second push has nothing to do and no notice"
OUT=$(sync_agr "$HOME_A" push --yes 2>&1)
if echo "$OUT" | grep -q "Nothing to push" && ! echo "$OUT" | grep -q "anyone with the link"; then
    pass "second push quiet"
else
    fail "second push wrong: $OUT"
fi

test_header "second machine needs the key, then pulls the session"
OUT=$(sync_agr "$HOME_B" pull --yes 2>&1) && RC=0 || RC=$?
PHRASE=$(sync_agr "$HOME_A" key export | grep -o 'agr1-[A-Za-z0-9-]*' | head -1)
if [[ $RC -ne 0 ]] && echo "$OUT" | grep -q "agr sync key import" && [[ -n "$PHRASE" ]]; then
    pass "pull without key aborts with the import hint"
else
    fail "pull without key wrong ($RC): $OUT"
fi
sync_agr "$HOME_B" key import "$PHRASE" >/dev/null 2>&1
OUT=$(sync_agr "$HOME_B" pull --yes 2>&1)
if echo "$OUT" | grep -q "Pulled: 1" \
   && cmp -s "$HOME_A/sessions/claude/one.cast.gz" "$HOME_B/sessions/claude/one.cast.gz"; then
    pass "pulled file is byte-identical"
else
    fail "pull wrong: $OUT"
fi

test_header "conflict is skipped, pull --overwrite keeps a backup"
sync_session "$HOME_A" claude one.cast.gz "changed on A"
sync_session "$HOME_B" claude one.cast.gz "changed on B"
sync_agr "$HOME_A" push --yes >/dev/null 2>&1
OUT=$(sync_agr "$HOME_B" push --yes 2>&1)
sync_agr "$HOME_B" pull --yes --overwrite >/dev/null 2>&1
if echo "$OUT" | grep -q "Conflicts: 1" && [[ -f "$HOME_B/sessions/claude/.one.cast.gz.bak" ]] \
   && cmp -s "$HOME_A/sessions/claude/one.cast.gz" "$HOME_B/sessions/claude/one.cast.gz"; then
    pass "conflict reported, overwrite pulled with .bak"
else
    fail "conflict handling wrong: $OUT"
fi

test_header "delete removes the remote copy and keeps the local file"
sync_agr "$HOME_A" delete --yes claude/one.cast.gz >/dev/null 2>&1
if ! ls "$SYNC_REMOTE"/*.blob >/dev/null 2>&1 && [[ -f "$HOME_A/sessions/claude/one.cast.gz" ]]; then
    pass "remote gone, local kept"
else
    fail "delete wrong"
fi

test_header "auto push after a recording"
sync_agr "$HOME_A" push --yes >/dev/null 2>&1
mkdir -p "$SYNC_ROOT/fakebin"
for tool in gh git; do printf '#!/bin/sh\nexit 0\n' > "$SYNC_ROOT/fakebin/$tool"; chmod +x "$SYNC_ROOT/fakebin/$tool"; done
BEFORE=$(ls "$SYNC_REMOTE"/*.blob 2>/dev/null | wc -l | tr -d ' ')
HOME="$HOME_A" XDG_CONFIG_HOME="$HOME_A/.config" PATH="$SYNC_ROOT/fakebin:$PATH" \
    AGR_SYNC_TEST_BACKEND_DIR="$SYNC_REMOTE" "$AGR" record echo -- "auto push me" </dev/null >/dev/null 2>&1 && RC=0 || RC=$?
for _ in $(seq 1 50); do
    AFTER=$(ls "$SYNC_REMOTE"/*.blob 2>/dev/null | wc -l | tr -d ' ')
    [[ "$AFTER" -gt "$BEFORE" ]] && break
    sleep 0.2
done
if [[ $RC -eq 0 && "$AFTER" -gt "$BEFORE" ]]; then
    pass "recording exit 0 and the session was pushed in the background"
else
    fail "auto push did not run (rc=$RC, before=$BEFORE, after=$AFTER)"
fi

test_header "auto push off keeps the session local"
printf '[sync]\nbackend = "gist"\nauto_push = false\n' >> "$HOME_A/.config/agr/config.toml"
BEFORE=$(ls "$SYNC_REMOTE"/*.blob | wc -l | tr -d ' ')
HOME="$HOME_A" XDG_CONFIG_HOME="$HOME_A/.config" PATH="$SYNC_ROOT/fakebin:$PATH" \
    AGR_SYNC_TEST_BACKEND_DIR="$SYNC_REMOTE" "$AGR" record echo -- "stay local" </dev/null >/dev/null 2>&1 || true
sleep 1
AFTER=$(ls "$SYNC_REMOTE"/*.blob | wc -l | tr -d ' ')
if [[ "$AFTER" -eq "$BEFORE" ]]; then
    pass "no upload with auto_push = false"
else
    fail "auto_push = false still uploaded"
fi
