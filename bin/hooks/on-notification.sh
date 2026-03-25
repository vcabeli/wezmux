#!/bin/bash
# wezmux hook: Claude Code Notification event
# Emits OSC 7777 structured message AND OSC 9 notification to terminal pane.
# OSC 7777 feeds the agent status store (structured status).
# OSC 9 feeds the notification store (latest_notification for sidebar preview).
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

# Read all of stdin (Claude Code sends JSON)
input=$(cat 2>/dev/null)

# Extract message from hook input
msg=""
if command -v jq >/dev/null 2>&1; then
    msg=$(echo "$input" | jq -r '
        .message //
        .notification.message //
        .notification.body //
        .body //
        .title //
        empty
    ' 2>/dev/null)
fi

[ -z "$msg" ] && msg="Claude notification"

# Strip escape/BEL to prevent OSC injection
msg=$(printf '%s' "$msg" | tr -d '\007\033')

# Emit both: structured status + notification store
printf '\033]7777;message;%s\007' "$msg" > /dev/tty 2>/dev/null || true
printf '\033]9;%s\007' "$msg" > /dev/tty 2>/dev/null || true
exit 0
