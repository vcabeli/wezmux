#!/bin/bash
# wezmux hook: Codex PreToolUse event
# Emits OSC 7777 tool name so sidebar can show what Codex is doing.

input=$(cat 2>/dev/null)

tool_name=""
thread_id="${CODEX_THREAD_ID:-${CODEX_SESSION_ID:-}}"
if command -v jq >/dev/null 2>&1; then
    tool_name=$(echo "$input" | jq -r '.tool_name // empty' 2>/dev/null)
    payload_thread_id=$(printf '%s' "$input" | jq -r '.session_id // .thread_id // empty' 2>/dev/null)
    [ -n "$payload_thread_id" ] && thread_id="$payload_thread_id"
fi

script_dir="$(cd "$(dirname "$0")" && pwd)"
"$script_dir/update-title.sh" "$thread_id" "${WEZMUX_TTY:-/dev/tty}" >/dev/null 2>&1 || true

if [ -n "$tool_name" ]; then
    # Strip escape/BEL to prevent OSC injection
    tool_name=$(printf '%s' "$tool_name" | tr -d '\007\033')
    printf '\033]7777;tool;%s\007' "$tool_name" > "${WEZMUX_TTY:-/dev/tty}" 2>/dev/null || true
fi
exit 0
