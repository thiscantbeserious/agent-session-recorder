#!/bin/bash
# Fake claude CLI for E2E testing.
#
# Accepts the same flags as the real Claude CLI and returns a valid
# marker response in the Claude wrapper JSON format. Reads and discards
# stdin (the prompt) so the pipe closes cleanly.

# Consume stdin to avoid broken pipe
cat > /dev/null

# Output a Claude CLI wrapper response with test markers
cat << 'EOF'
{"type":"result","subtype":"success","is_error":false,"result":"{\"markers\":[{\"timestamp\":0.5,\"label\":\"E2E test marker from fake claude\",\"category\":\"implementation\"}]}"}
EOF
