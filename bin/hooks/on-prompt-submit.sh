#!/bin/bash
# wezmux hook: Claude Code UserPromptSubmit event
# Emits OSC 7777 structured status AND OSC 9 notification to terminal pane.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

. "$(dirname "$0")/lib/agent-tty.sh"
agent_tty=$(wezmux_agent_tty) || exit 0

printf '\033]7777;status;working\007' > "$agent_tty" 2>/dev/null || true
printf '\033]9;Claude is working...\007' > "$agent_tty" 2>/dev/null || true
exit 0
