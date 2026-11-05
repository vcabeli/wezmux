#!/bin/bash
# wezmux hook: Claude Code StopFailure event
# A turn that ends on an API error (rate limit, overload, auth) fires
# StopFailure instead of Stop; without this the card stays on Working.

input=$(cat 2>/dev/null)

. "$(dirname "$0")/lib/agent-tty.sh"
agent_tty=$(wezmux_agent_tty) || exit 0

error=""
if command -v jq >/dev/null 2>&1; then
    error=$(echo "$input" | jq -r '.error // empty' 2>/dev/null)
fi
msg="Claude stopped on an error${error:+ ($error)}"

# Strip escape/BEL/semicolons to prevent OSC injection
msg=$(printf '%s' "$msg" | tr -d '\007\033;')

printf '\033]7777;status;idle\007' > "$agent_tty" 2>/dev/null || true
printf '\033]7777;message;%s\007' "$msg" > "$agent_tty" 2>/dev/null || true
printf '\033]9;%s\007' "$msg" > "$agent_tty" 2>/dev/null || true
exit 0
