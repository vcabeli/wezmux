#!/bin/bash
# wezmux hook: Claude Code PreToolUse for AskUserQuestion
# Emits OSC 9 "needs input" notification to terminal pane.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json
# Uses PreToolUse matcher "AskUserQuestion"

printf '\033]9;Claude is waiting for your input\007' > /dev/tty 2>/dev/null || true
exit 0
