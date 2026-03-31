#!/bin/bash
# wezmux hook: Codex Stop event
# Emits OSC 7777 structured status AND OSC 9 notification to terminal pane.

input=$(cat 2>/dev/null)

last_message=""
if command -v jq >/dev/null 2>&1; then
    last_message=$(echo "$input" | jq -r '.last_assistant_message // empty' 2>/dev/null)
fi

msg="Codex finished"

# Use Codex's actual response as preview, truncated to ~200 chars
preview="$msg"
if [ -n "$last_message" ]; then
    preview=$(echo "$last_message" | tr '\n' ' ' | sed 's/  */ /g' | cut -c1-200)
fi

# Strip escape/BEL to prevent OSC injection
preview=$(printf '%s' "$preview" | tr -d '\007\033')

# Structured status for agent store
printf '\033]7777;status;idle\007' > /dev/tty 2>/dev/null || true
printf '\033]7777;message;%s\007' "$preview" > /dev/tty 2>/dev/null || true
# Notification store
printf '\033]9;%s\007' "$msg" > /dev/tty 2>/dev/null || true
exit 0
