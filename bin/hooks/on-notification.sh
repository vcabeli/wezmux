#!/bin/bash
# wezmux hook: Claude Code Notification event
# Emits OSC 9 to the terminal pane so wezmux sidebar picks it up.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json
# See: ~/.claude/settings.json "Notification" hook entry

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

# Emit OSC 9 to the terminal (bypasses stdout capture)
printf '\033]9;%s\007' "$msg" > /dev/tty 2>/dev/null || true
exit 0
