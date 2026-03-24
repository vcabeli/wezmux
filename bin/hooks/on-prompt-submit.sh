#!/bin/bash
# wezmux hook: Claude Code UserPromptSubmit event
# Emits OSC 9 "working" notification to terminal pane.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

printf '\033]9;Claude is working...\007' > /dev/tty 2>/dev/null || true
exit 0
