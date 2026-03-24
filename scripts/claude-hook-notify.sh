#!/bin/bash
# Wezmux notification hook for Claude Code
# Add to ~/.claude/settings.json hooks to get sidebar status updates
#
# Usage in settings.json:
#   "hooks": {
#     "Stop": [{ "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux-notify.sh idle" }] }],
#     "PreToolUse": [
#       { "matcher": "AskUserQuestion", "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux-notify.sh needs_input" }] }
#     ]
#   }

case "$1" in
  needs_input|waiting)
    printf '\033]9;Claude is waiting for your input\a'
    ;;
  idle|finished|done)
    printf '\033]9;Claude finished\a'
    ;;
  working|running)
    printf '\033]9;Claude is working\a'
    ;;
  *)
    printf '\033]9;%s\a' "$*"
    ;;
esac
