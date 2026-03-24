#!/bin/bash
# wezmux hook: Claude Code Stop event
# Emits OSC 9 "finished" notification to terminal pane.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

input=$(cat 2>/dev/null)

reason="end_turn"
if command -v jq >/dev/null 2>&1; then
    reason=$(echo "$input" | jq -r '.stop_hook_reason // .reason // "end_turn"' 2>/dev/null)
fi

case "$reason" in
    end_turn)       msg="Claude finished" ;;
    stop_button)    msg="Claude stopped by user" ;;
    interrupt)      msg="Claude interrupted" ;;
    *)              msg="Claude finished ($reason)" ;;
esac

printf '\033]9;%s\007' "$msg" > /dev/tty 2>/dev/null || true
exit 0
