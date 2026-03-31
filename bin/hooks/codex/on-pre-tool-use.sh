#!/bin/bash
# wezmux hook: Codex PreToolUse event
# Emits OSC 7777 tool name so sidebar can show what Codex is doing.

input=$(cat 2>/dev/null)

tool_name=""
if command -v jq >/dev/null 2>&1; then
    tool_name=$(echo "$input" | jq -r '.tool_name // empty' 2>/dev/null)
fi

if [ -n "$tool_name" ]; then
    # Strip escape/BEL to prevent OSC injection
    tool_name=$(printf '%s' "$tool_name" | tr -d '\007\033')
    printf '\033]7777;tool;%s\007' "$tool_name" > /dev/tty 2>/dev/null || true
fi
exit 0
