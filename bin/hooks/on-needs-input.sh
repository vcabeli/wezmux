#!/bin/bash
# wezmux hook: Claude Code PreToolUse for AskUserQuestion
# Emits OSC 7777 structured status AND OSC 9 notification to terminal pane.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

printf '\033]7777;status;needs_input\007' > /dev/tty 2>/dev/null || true
printf '\033]9;Claude is waiting for your input\007' > /dev/tty 2>/dev/null || true
exit 0
